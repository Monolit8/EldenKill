namespace EldenKill
{
    // Byte layout of the EldenKill link (protocol/eldenkill_protocol.h, version 1). The host half
    // (host-eldenring/src/proto.rs) uses the same numbers: change all three together.
    internal static class Proto
    {
        public const uint Magic = 0x314C4B45;
        public const uint Version = 1;
        public const string MappingName = "Local\\EldenKill_v1";
        public const string GuestMutex = "Local\\EldenKill_v1_guest";

        public const long OffHeader = 0x0;
        public const long OffHostState = 0x100;
        public const long OffGuestState = 0x200;
        public const long OffOverlayCtl = 0x300;
        public const long OffOverlayHdr = 0x340;
        public const long OffStyleText = 0x800;
        public const long OffHudPrefs = 0xC00;
        public const long OffInputRing = 0x1000;
        public const long OffActorTable = 0x12000;
        public const long OffEventRing = 0x17000;
        public const long OffCollision = 0x20000;
        public const long CollisionBytes = 32L << 20;
        public const long OffOverlayPixels = OffCollision + CollisionBytes;
        public const int OverlayMaxW = 3840;
        public const int OverlayMaxH = 2160;
        public const long OverlaySlotBytes = (long)OverlayMaxW * OverlayMaxH * 4;
        public const int OverlaySlots = 3;
        public const long MappingBytes = OffOverlayPixels + OverlaySlotBytes * OverlaySlots;

        // Header
        public const long HMagic = 0x00, HVersion = 0x04, HHostPid = 0x08, HGuestPid = 0x0C, HHostBeat = 0x10, HGuestBeat = 0x18;

        // HostState (host -> guest), seqlock
        public const long HsSeq = 0, HsFlags = 4, HsWorldId = 8, HsEpoch = 12, HsOrigin = 16, HsPos = 40, HsTeleportSeq = 52,
            HsViewportW = 56, HsViewportH = 60, HsUnitsPerMetre = 64, HsHp = 68, HsHpMax = 72, HsYaw = 76, HsGameHour = 80;
        public const uint HostInWorld = 1, HostMenu = 2, HostLoading = 4, HostDead = 8, HostCutscene = 16, HostEnabled = 32,
            HostShowCollision = 64, HostShowHitboxes = 128;

        // GuestState (guest -> host), seqlock
        public const long GsSeq = 0, GsFlags = 4, GsTeleportAck = 8, GsEpochAck = 12, GsPos = 16, GsVel = 28, GsEye = 40,
            GsFwd = 52, GsUp = 64, GsFov = 76, GsHp = 80, GsHardDamage = 84, GsStamina = 88, GsWeapon = 92, GsStyleRank = 96,
            GsFrame = 100, GsFrameQpc = 104;
        public const uint GuestInLevel = 1, GuestOnGround = 2, GuestDead = 4, GuestSliding = 8, GuestDashing = 16,
            GuestJumping = 32, GuestDriving = 64, GuestMenu = 128, GuestNoGround = 256;

        // Overlay triple buffer
        public const int OverlayDirty = 4;
        public const long OcState = 0, OcFrames = 8;
        public const long SlotHdrBytes = 0x40, ShWidth = 0, ShHeight = 4, ShFlags = 8, ShFrameId = 16;
        public const uint OvBottomUp = 1, OvChromaKey = 2;

        // Rings
        public const long RingHead = 0x00, RingTail = 0x40, RingData = 0x80;

        // Input ring (host -> guest)
        public const int InputEntries = 4096;
        public const ushort InKey = 1, InMouseButton = 2, InMouseMove = 3, InScroll = 4, InRelease = 5, InHurt = 6, InKilled = 7, InMousePos = 8;
        public const ushort HurtMelee = 0, HurtProjectile = 1, HurtMagic = 2, HurtFall = 3, HurtOther = 4;

        // Actor table (host -> guest), seqlock
        public const int MaxActors = 256;
        public const long AtSeq = 0, AtCount = 4, AtRecords = 0x40, ActorBytes = 64;
        public const uint ActorHostile = 1, ActorDead = 2, ActorBoss = 4;

        // Event ring (guest -> host)
        public const int EventEntries = 512;
        public const long EventBytes = 32;
        public const uint EvHitActor = 1, EvDied = 2, EvExplosion = 3, EvParry = 4, EvHeal = 5;
        public const uint HitProjectile = 1, HitExplosion = 2, HitMelee = 4, HitHeadshot = 8, HitParry = 16;

        // Collision ring (host -> guest)
        public const long ColDataBytes = CollisionBytes - RingData;
        public const uint ColPad = 0, ColClear = 1, ColRegion = 2, ColRemove = 3;
        public const uint TriWalkable = 1, TriWater = 2, TriLava = 4;
        public const int ColTriBytes = 40;
    }
}
