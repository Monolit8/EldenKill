/*
 * EldenKill link protocol, version 1.
 *
 * One named shared-memory mapping between the two halves of EldenKill:
 *   host  = Elden Ring  (host-eldenring, a Rust DLL loaded by me3)       creates the mapping
 *   guest = ULTRAKILL   (guest-ultrakill, a BepInEx plugin, runs hidden)  opens it
 *
 * Same shape as SkyCraft's protocol (which Killcraft speaks), with the roles swapped:
 * in Killcraft ULTRAKILL is the host world and Minecraft simulates the player; here Elden Ring is
 * the host world and ULTRAKILL simulates the player (V1).
 *
 * This file is documentation and the single source of truth. Its mirrors:
 *   host-eldenring/src/proto.rs      guest-ultrakill/src/Proto.cs
 * Change all three together and bump EK_VERSION.
 *
 * Conventions
 *  - Little endian, natural alignment. All offsets are bytes from the start of the mapping.
 *  - "seqlock" blocks: the writer bumps seq to odd, writes, bumps to even. Readers retry while seq
 *    is odd or changed under them, and keep last frame's copy on a torn read.
 *  - Rings: single producer, single consumer. head (i64, producer) at +0x00, tail (i64, consumer)
 *    at +0x40, data at +0x80. Fixed-size rings index entries by (counter & (entries - 1));
 *    byte rings hold 8-aligned [u32 type][u32 payload bytes][payload] messages, and a PAD message
 *    (type 0) means "skip to the start of the ring".
 *  - Coordinates in the protocol are in the GUEST's space (Unity units, left-handed, Y up),
 *    relative to HostState.origin: the host converts, as Killcraft's host converts to Minecraft's.
 *    Elden Ring's Havok space is also left-handed Y-up (er-mario: "SM64 and Havok are mirrored on
 *    X"), so the conversion is   unity = (havok - origin) * units_per_metre   and back.
 *  - Heartbeats are GetTickCount64() milliseconds; a side silent for 3 s is gone.
 */
#pragma once
#include <stdint.h>

#define EK_MAGIC          0x314C4B45u   /* "EKL1" */
#define EK_VERSION        1u
#define EK_MAPPING_NAME   "Local\\EldenKill_v1"
/* A named mutex the guest holds while it runs (the host's launcher checks it). */
#define EK_GUEST_MUTEX    "Local\\EldenKill_v1_guest"

/* ---- layout ---------------------------------------------------------------------------------- */
#define EK_OFF_HEADER       0x00000
#define EK_OFF_HOST_STATE   0x00100
#define EK_OFF_GUEST_STATE  0x00200
#define EK_OFF_OVERLAY_CTL  0x00300
#define EK_OFF_OVERLAY_HDR  0x00340   /* 3 slot headers, 0x40 each */
#define EK_OFF_INPUT_RING   0x01000
#define EK_OFF_ACTOR_TABLE  0x12000
#define EK_OFF_EVENT_RING   0x17000
#define EK_OFF_COLLISION    0x20000
#define EK_COLLISION_BYTES  (32ull << 20)
#define EK_OFF_OVERLAY_PIX  (EK_OFF_COLLISION + EK_COLLISION_BYTES)
#define EK_OVERLAY_MAX_W    3840
#define EK_OVERLAY_MAX_H    2160
#define EK_OVERLAY_SLOT     ((uint64_t)EK_OVERLAY_MAX_W * EK_OVERLAY_MAX_H * 4)
#define EK_OVERLAY_SLOTS    3
#define EK_MAPPING_BYTES    (EK_OFF_OVERLAY_PIX + EK_OVERLAY_SLOT * EK_OVERLAY_SLOTS)

/* ---- header ---------------------------------------------------------------------------------- */
typedef struct {
    uint32_t magic;        /* +0x00 written last by the host: the mapping is ready */
    uint32_t version;      /* +0x04 */
    uint32_t host_pid;     /* +0x08 */
    uint32_t guest_pid;    /* +0x0C */
    uint64_t host_beat;    /* +0x10 */
    uint64_t guest_beat;   /* +0x18 */
} EkHeader;

/* ---- host -> guest: where the Tarnished is and what Elden Ring is doing (seqlock) ------------ */
enum {
    EK_HOST_IN_WORLD  = 1,   /* a character is loaded and the world runs */
    EK_HOST_MENU      = 2,   /* a menu / popup has the controls: V1 stands still */
    EK_HOST_LOADING   = 4,   /* loading screen, fast travel, warp: V1 is frozen */
    EK_HOST_DEAD      = 8,   /* the Tarnished died ("YOU DIED"): V1 dies too */
    EK_HOST_CUTSCENE  = 16,  /* cutscene / scripted animation (ladders, doors, graces): host has the player */
    EK_HOST_ENABLED   = 32,  /* V1 mode is on (F9 toggles it) */
    EK_HOST_SHOW_COLLISION = 64,  /* debug (Num1): draw Elden Ring's collision in the overlay */
    EK_HOST_SHOW_HITBOXES  = 128, /* debug (Num2): draw the enemy hitboxes */
};
typedef struct {
    uint32_t seq;              /* +0  */
    uint32_t flags;            /* +4  EK_HOST_* */
    uint32_t world_id;         /* +8  Elden Ring's map id (area/block) */
    uint32_t epoch;            /* +12 bumped when collision restarts (new origin, new area, reconnect) */
    double   origin[3];        /* +16 Havok position (metres) of the guest's (0,0,0) for this epoch */
    float    pos[3];           /* +40 the Tarnished's feet, guest units */
    uint32_t teleport_seq;     /* +52 bumped when the host moved the player itself: guest puts V1 at pos */
    uint32_t viewport_w;       /* +56 Elden Ring's back buffer: the overlay is rendered at this size */
    uint32_t viewport_h;       /* +60 */
    float    units_per_metre;  /* +64 */
    int32_t  hp, hp_max;       /* +68 the Tarnished's HP (for death / revive sync) */
    float    yaw;              /* +76 facing after a teleport, Unity degrees (0 = +Z) */
    float    game_hour;        /* +80 */
} EkHostState;                 /* 0x60 */

/* ---- guest -> host: V1 (seqlock) -------------------------------------------------------------- */
enum {
    EK_GUEST_IN_LEVEL  = 1,    /* the guest's arena scene is up and V1 exists */
    EK_GUEST_ON_GROUND = 2,
    EK_GUEST_DEAD      = 4,
    EK_GUEST_SLIDING   = 8,
    EK_GUEST_DASHING   = 16,
    EK_GUEST_JUMPING   = 32,
    EK_GUEST_DRIVING   = 64,   /* V1 is under the link (not waiting for collision / a teleport) */
    EK_GUEST_MENU      = 128,  /* ULTRAKILL's own menu (pause / options) is open */
    EK_GUEST_NO_GROUND = 256,  /* no Elden Ring collision at all within 200 m under V1 */
};
typedef struct {
    uint32_t seq;              /* +0   */
    uint32_t flags;            /* +4   EK_GUEST_* */
    uint32_t teleport_ack;     /* +8   last HostState.teleport_seq V1 was put at */
    uint32_t epoch_ack;        /* +12  last epoch whose CLEAR the guest consumed */
    float    pos[3];           /* +16  V1's feet, guest units, relative to the acked epoch's origin */
    float    vel[3];           /* +28  units/s */
    float    eye[3];           /* +40  camera position */
    float    fwd[3];           /* +52  camera forward (unit) */
    float    up[3];            /* +64  camera up (unit) */
    float    fov;              /* +76  vertical fov, degrees */
    int32_t  hp;               /* +80  0..100 (200 with overheal) */
    float    hard_damage;      /* +84  */
    float    stamina;          /* +88  dash meter, 0..300 (100 per dash) */
    uint32_t weapon;           /* +92  gun slot */
    uint32_t style_rank;       /* +96  */
    uint32_t frame;            /* +100 */
    int64_t  frame_qpc;        /* +104 QueryPerformanceCounter when this was written */
} EkGuestState;                /* 0x70 */

/* ---- guest -> host: V1's view as an RGBA overlay (triple buffer) ----------------------------- */
/* ctl.state: bits 0-1 = the slot holding the newest finished frame, bit 2 = it is unread.
 *   guest: renders into its back slot b, then b = xchg(state, b | DIRTY) & 3
 *   host:  if state & DIRTY: front = xchg(state, front) & 3, then reads slot front
 * The host resets state = 1, front = 2 for every new guest process; the guest starts with b = 0. */
#define EK_OVERLAY_DIRTY  4u
enum { EK_OV_BOTTOM_UP = 1, EK_OV_CHROMA_KEY = 2 /* key colour = 0xFF00FF, alpha ignored */ };
typedef struct { int32_t state; uint32_t pad; uint64_t frames; } EkOverlayCtl;
typedef struct { uint32_t width, height, flags, pad; uint64_t frame_id; } EkOverlaySlotHdr;

/* ---- host -> guest: input (fixed ring, 4096 x 16 bytes) --------------------------------------- */
#define EK_INPUT_ENTRIES 4096
enum {
    EK_IN_KEY        = 1,  /* code = Windows virtual key, a = 1 down / 0 up */
    EK_IN_MOUSE_BTN  = 2,  /* code = 0 left, 1 right, 2 middle, 3 back, 4 forward; a = down */
    EK_IN_MOUSE_MOVE = 3,  /* a = dx, b = dy (raw mouse counts, DirectInput lX / lY) */
    EK_IN_SCROLL     = 4,  /* a = wheel delta (120 per notch) */
    EK_IN_RELEASE    = 5,  /* release every key and button (focus lost, menu opened) */
    EK_IN_MOUSE_POS  = 8,  /* a = x, b = y on the host's picture, 0..10000 from the top left (menus) */
    EK_IN_KILLED     = 7,  /* an Elden Ring enemy V1 hit died: a = 1 for a boss, b = actor id (style, blood, healing) */
    EK_IN_HURT       = 6,  /* a = damage (Elden Ring HP lost), b = source yaw * 100, code = EK_HURT_* */
};
enum { EK_HURT_MELEE = 0, EK_HURT_PROJECTILE = 1, EK_HURT_MAGIC = 2, EK_HURT_FALL = 3, EK_HURT_OTHER = 4 };
typedef struct { uint16_t type, code; int32_t a, b, c; } EkInput;

/* ---- host -> guest: Elden Ring characters near the player (seqlock) -------------------------- */
#define EK_MAX_ACTORS 256
enum { EK_ACTOR_HOSTILE = 1, EK_ACTOR_DEAD = 2, EK_ACTOR_BOSS = 4 };
typedef struct {               /* 64 bytes, records start at +0x40 */
    uint32_t id;               /* +0  low 32 bits of the FieldInsHandle */
    uint32_t flags;            /* +4  */
    float    pos[3];           /* +8  feet, guest units */
    float    yaw;              /* +20 */
    float    radius, height;   /* +24 guest units */
    float    hp_frac;          /* +32 */
    uint16_t team, pad;        /* +36 */
    char     name[24];         /* +40 */
} EkActor;
/* table: u32 seq at +0, u32 count at +4, EkActor[EK_MAX_ACTORS] at +0x40 */

/* ---- guest -> host: what V1 did (fixed ring, 512 x 32 bytes) ---------------------------------- */
#define EK_EVENT_ENTRIES 512
enum {
    EK_EV_HIT_ACTOR = 1,   /* actor = id, a = ULTRAKILL damage, b c d = hit point (guest units) */
    EK_EV_DIED      = 2,   /* V1 died: the Tarnished dies */
    EK_EV_EXPLOSION = 3,   /* a = radius, b c d = centre; weapon = damage x 100 (host damages actors) */
    EK_EV_PARRY     = 4,   /* actor = id */
    EK_EV_HEAL      = 5,   /* a = hp gained */
};
enum { EK_HIT_PROJECTILE = 1, EK_HIT_EXPLOSION = 2, EK_HIT_MELEE = 4, EK_HIT_HEADSHOT = 8, EK_HIT_PARRY = 16 };
typedef struct { uint32_t type, actor; float a, b, c, d; uint32_t flags, weapon; } EkEvent;

/* ---- host -> guest: collision (byte ring, 32 MB) ---------------------------------------------- */
/* A region is one Havok body's triangles, in guest units for the current epoch's origin.
 * The guest builds one MeshCollider per region on ULTRAKILL's Environment layer (8). */
enum { EK_COL_PAD = 0, EK_COL_CLEAR = 1, EK_COL_REGION = 2, EK_COL_REMOVE = 3 };
enum { EK_TRI_WALKABLE = 1, EK_TRI_WATER = 2, EK_TRI_LAVA = 4 };
/* triangle flags bits 8-31: the Havok body it comes from (one region mixes many bodies) */
/* CLEAR:  u32 epoch
 * REGION: u64 id, u32 epoch, u32 tri_count, then tri_count x { float v[9]; u32 flags; } (40 bytes)
 * REMOVE: u64 id, u32 epoch, u32 pad */

/* guest -> host: ULTRAKILL's style feed ("+ HEADSHOT"...), at 0x800: u32 seq, u32 len, then
 * len bytes of UTF-8 with TextMeshPro tags (max 1016). seq goes up after each write. */
#define EK_OFF_STYLE_TEXT 0x800

/* guest -> host: ULTRAKILL's HUD settings, at 0xC00: u32 valid, i32 hudType (0 = no HUD),
 * i32 crossHair (0 none, 1, 2), i32 crossHairColor (0 inverted, 1 white, 2 grey, 3 black, 4 red,
 * 5 green, 6 blue, 7 cyan, 8 yellow, 9 magenta), i32 crossHairHud (0 off), i32 styleMeter, f32 hudBackgroundOpacity (0-100) */
#define EK_OFF_HUD_PREFS 0xC00
