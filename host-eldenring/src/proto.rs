//! Byte layout of the EldenKill link (protocol/eldenkill_protocol.h, version 1). The guest half
//! (guest-ultrakill/src/Proto.cs) uses the same numbers: change all three together.
#![allow(dead_code)]

pub const MAGIC: u32 = 0x314C_4B45;
pub const VERSION: u32 = 1;
pub const MAPPING_NAME: &str = "Local\\EldenKill_v1";
pub const GUEST_MUTEX: &str = "Local\\EldenKill_v1_guest";

pub const OFF_HEADER: usize = 0x0;
pub const OFF_HOST_STATE: usize = 0x100;
pub const OFF_GUEST_STATE: usize = 0x200;
pub const OFF_OVERLAY_CTL: usize = 0x300;
pub const OFF_OVERLAY_HDR: usize = 0x340;
pub const OFF_STYLE_TEXT: usize = 0x800;
pub const OFF_HUD_PREFS: usize = 0xC00;
pub const OFF_INPUT_RING: usize = 0x1000;
pub const OFF_ACTOR_TABLE: usize = 0x12000;
pub const OFF_EVENT_RING: usize = 0x17000;
pub const OFF_COLLISION: usize = 0x20000;
pub const COLLISION_BYTES: usize = 32 << 20;
pub const OFF_OVERLAY_PIXELS: usize = OFF_COLLISION + COLLISION_BYTES;
pub const OVERLAY_MAX_W: usize = 3840;
pub const OVERLAY_MAX_H: usize = 2160;
pub const OVERLAY_SLOT_BYTES: usize = OVERLAY_MAX_W * OVERLAY_MAX_H * 4;
pub const OVERLAY_SLOTS: usize = 3;
pub const MAPPING_BYTES: usize = OFF_OVERLAY_PIXELS + OVERLAY_SLOT_BYTES * OVERLAY_SLOTS;

// header
pub const H_MAGIC: usize = 0x00;
pub const H_VERSION: usize = 0x04;
pub const H_HOST_PID: usize = 0x08;
pub const H_GUEST_PID: usize = 0x0C;
pub const H_HOST_BEAT: usize = 0x10;
pub const H_GUEST_BEAT: usize = 0x18;

// HostState (host -> guest), seqlock
pub const HS_SEQ: usize = 0;
pub const HS_FLAGS: usize = 4;
pub const HS_WORLD_ID: usize = 8;
pub const HS_EPOCH: usize = 12;
pub const HS_ORIGIN: usize = 16;
pub const HS_POS: usize = 40;
pub const HS_TELEPORT_SEQ: usize = 52;
pub const HS_VIEWPORT_W: usize = 56;
pub const HS_VIEWPORT_H: usize = 60;
pub const HS_UNITS_PER_METRE: usize = 64;
pub const HS_HP: usize = 68;
pub const HS_HP_MAX: usize = 72;
pub const HS_YAW: usize = 76;
pub const HS_GAME_HOUR: usize = 80;
pub const HOST_IN_WORLD: u32 = 1;
pub const HOST_MENU: u32 = 2;
pub const HOST_LOADING: u32 = 4;
pub const HOST_DEAD: u32 = 8;
pub const HOST_CUTSCENE: u32 = 16;
pub const HOST_ENABLED: u32 = 32;
pub const HOST_SHOW_COLLISION: u32 = 64;
pub const HOST_SHOW_HITBOXES: u32 = 128;

// GuestState (guest -> host), seqlock
pub const GS_SEQ: usize = 0;
pub const GS_FLAGS: usize = 4;
pub const GS_TELEPORT_ACK: usize = 8;
pub const GS_EPOCH_ACK: usize = 12;
pub const GS_POS: usize = 16;
pub const GS_VEL: usize = 28;
pub const GS_EYE: usize = 40;
pub const GS_FWD: usize = 52;
pub const GS_UP: usize = 64;
pub const GS_FOV: usize = 76;
pub const GS_HP: usize = 80;
pub const GS_HARD_DAMAGE: usize = 84;
pub const GS_STAMINA: usize = 88;
pub const GS_WEAPON: usize = 92;
pub const GS_STYLE_RANK: usize = 96;
pub const GS_FRAME: usize = 100;
pub const GS_FRAME_QPC: usize = 104;
pub const GUEST_IN_LEVEL: u32 = 1;
pub const GUEST_ON_GROUND: u32 = 2;
pub const GUEST_DEAD: u32 = 4;
pub const GUEST_SLIDING: u32 = 8;
pub const GUEST_DASHING: u32 = 16;
pub const GUEST_JUMPING: u32 = 32;
pub const GUEST_DRIVING: u32 = 64;
pub const GUEST_MENU: u32 = 128;
pub const GUEST_NO_GROUND: u32 = 256;

// overlay triple buffer
pub const OVERLAY_DIRTY: i32 = 4;
pub const OC_STATE: usize = 0;
pub const OC_FRAMES: usize = 8;
pub const SLOT_HDR_BYTES: usize = 0x40;
pub const SH_WIDTH: usize = 0;
pub const SH_HEIGHT: usize = 4;
pub const SH_FLAGS: usize = 8;
pub const SH_FRAME_ID: usize = 16;
pub const OV_BOTTOM_UP: u32 = 1;
pub const OV_CHROMA_KEY: u32 = 2;

// rings
pub const RING_HEAD: usize = 0x00;
pub const RING_TAIL: usize = 0x40;
pub const RING_DATA: usize = 0x80;

// input ring (host -> guest)
pub const INPUT_ENTRIES: usize = 4096;
pub const IN_KEY: u16 = 1;
pub const IN_MOUSE_BUTTON: u16 = 2;
pub const IN_MOUSE_MOVE: u16 = 3;
pub const IN_SCROLL: u16 = 4;
pub const IN_RELEASE: u16 = 5;
pub const IN_HURT: u16 = 6;
pub const IN_KILLED: u16 = 7;
/// a = x, b = y on Elden Ring's picture, 0-10000 from the top left (ULTRAKILL's menus)
pub const IN_MOUSE_POS: u16 = 8;
pub const HURT_MELEE: u16 = 0;
pub const HURT_PROJECTILE: u16 = 1;
pub const HURT_MAGIC: u16 = 2;
pub const HURT_FALL: u16 = 3;
pub const HURT_OTHER: u16 = 4;

// actor table (host -> guest), seqlock
pub const MAX_ACTORS: usize = 256;
pub const AT_SEQ: usize = 0;
pub const AT_COUNT: usize = 4;
pub const AT_RECORDS: usize = 0x40;
pub const ACTOR_BYTES: usize = 64;
pub const ACTOR_HOSTILE: u32 = 1;
pub const ACTOR_DEAD: u32 = 2;
pub const ACTOR_BOSS: u32 = 4;

// event ring (guest -> host)
pub const EVENT_ENTRIES: usize = 512;
pub const EVENT_BYTES: usize = 32;
pub const EV_HIT_ACTOR: u32 = 1;
pub const EV_DIED: u32 = 2;
pub const EV_EXPLOSION: u32 = 3;
pub const EV_PARRY: u32 = 4;
pub const EV_HEAL: u32 = 5;
pub const HIT_PROJECTILE: u32 = 1;
pub const HIT_EXPLOSION: u32 = 2;
pub const HIT_MELEE: u32 = 4;
pub const HIT_HEADSHOT: u32 = 8;
pub const HIT_PARRY: u32 = 16;

// collision ring (host -> guest)
pub const COL_DATA_BYTES: usize = COLLISION_BYTES - RING_DATA;
pub const COL_PAD: u32 = 0;
pub const COL_CLEAR: u32 = 1;
pub const COL_REGION: u32 = 2;
pub const COL_REMOVE: u32 = 3;
pub const TRI_WALKABLE: u32 = 1;
pub const TRI_WATER: u32 = 2;
pub const TRI_LAVA: u32 = 4;
pub const COL_TRI_BYTES: usize = 40;
