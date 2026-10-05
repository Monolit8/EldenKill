using System;
using System.Runtime.InteropServices;
using System.Threading;

namespace EldenKill
{
    internal struct HostState
    {
        public uint Flags, WorldId, Epoch, TeleportSeq, ViewportW, ViewportH;
        public double OriginX, OriginY, OriginZ;
        public float X, Y, Z, UnitsPerMetre, Yaw, GameHour;
        public int Hp, HpMax;
    }

    internal struct GuestState
    {
        public uint Flags, TeleportAck, EpochAck, Weapon, StyleRank, Frame;
        public UnityEngine.Vector3 Pos, Vel, Eye, Fwd, Up;
        public float Fov, HardDamage, Stamina;
        public int Hp;
    }

    internal struct InputEvent
    {
        public ushort Type, Code;
        public int A, B, C;
    }

    internal struct Actor
    {
        public uint Id, Flags;
        public UnityEngine.Vector3 Pos;
        public float Yaw, Radius, Height, HpFrac;
        public ushort Team;
        public string Name;
    }

    // The guest end of the link. Elden Ring's DLL creates the mapping (it plays the part Skyrim's
    // SKSE plugin plays in SkyCraft); ULTRAKILL opens it, the part Minecraft plays there.
    internal static unsafe class Link
    {
        private static IntPtr mapping;
        private static byte* b;
        private static IntPtr guestMutex;
        private static int overlayBack;

        public static bool Ready => b != null;

        // Opens the host's mapping if it exists and is initialised. Cheap to call every second.
        public static bool TryOpen()
        {
            if (b != null)
            {
                return true;
            }
            IntPtr m = Native.OpenFileMappingW(Native.FileMapAllAccess, false, Proto.MappingName);
            if (m == IntPtr.Zero)
            {
                return false;
            }
            IntPtr view = Native.MapViewOfFile(m, Native.FileMapAllAccess, 0, 0, UIntPtr.Zero);
            if (view == IntPtr.Zero)
            {
                Native.CloseHandle(m);
                return false;
            }
            byte* p = (byte*)view;
            if (Volatile.Read(ref *(uint*)(p + Proto.OffHeader + Proto.HMagic)) != Proto.Magic)
            {
                Native.UnmapViewOfFile(view);
                Native.CloseHandle(m);
                return false;
            }
            uint version = *(uint*)(p + Proto.HVersion);
            if (version != Proto.Version)
            {
                Plugin.Log.LogError($"Elden Ring speaks link version {version}, this plugin {Proto.Version}: update both halves");
                Native.UnmapViewOfFile(view);
                Native.CloseHandle(m);
                return false;
            }
            mapping = m;
            b = p;
            overlayBack = 0;
            U32(Proto.OffHeader + Proto.HGuestPid) = Native.GetCurrentProcessId();
            Heartbeat();
            if (guestMutex == IntPtr.Zero)
            {
                guestMutex = Native.CreateMutexW(IntPtr.Zero, false, Proto.GuestMutex);
            }
            Plugin.Log.LogInfo($"linked to Elden Ring (pid {HostPid()})");
            return true;
        }

        public static void Close()
        {
            if (b == null)
            {
                return;
            }
            Native.UnmapViewOfFile((IntPtr)b);
            Native.CloseHandle(mapping);
            b = null;
            mapping = IntPtr.Zero;
        }

        private static ref int I32(long off) => ref *(int*)(b + off);
        private static ref uint U32(long off) => ref *(uint*)(b + off);
        private static ref long I64(long off) => ref *(long*)(b + off);
        private static ref float F32(long off) => ref *(float*)(b + off);
        private static ref double F64(long off) => ref *(double*)(b + off);

        private static UnityEngine.Vector3 V3(long off) => new UnityEngine.Vector3(F32(off), F32(off + 4), F32(off + 8));

        private static void SetV3(long off, UnityEngine.Vector3 v)
        {
            F32(off) = v.x;
            F32(off + 4) = v.y;
            F32(off + 8) = v.z;
        }

        public static void Heartbeat()
        {
            if (b != null)
            {
                Volatile.Write(ref I64(Proto.OffHeader + Proto.HGuestBeat), (long)Native.GetTickCount64());
            }
        }

        public static bool HostAlive()
        {
            if (b == null)
            {
                return false;
            }
            long last = Volatile.Read(ref I64(Proto.OffHeader + Proto.HHostBeat));
            return last != 0 && (long)Native.GetTickCount64() - last < 3000;
        }

        public static uint HostPid() => b == null ? 0 : U32(Proto.OffHeader + Proto.HHostPid);

        public static bool ReadHostState(ref HostState s)
        {
            if (b == null)
            {
                return false;
            }
            long o = Proto.OffHostState;
            for (int attempt = 0; attempt < 64; attempt++)
            {
                int s1 = Volatile.Read(ref I32(o + Proto.HsSeq));
                if ((s1 & 1) != 0)
                {
                    Thread.SpinWait(8);
                    continue;
                }
                s.Flags = U32(o + Proto.HsFlags);
                s.WorldId = U32(o + Proto.HsWorldId);
                s.Epoch = U32(o + Proto.HsEpoch);
                s.OriginX = F64(o + Proto.HsOrigin);
                s.OriginY = F64(o + Proto.HsOrigin + 8);
                s.OriginZ = F64(o + Proto.HsOrigin + 16);
                s.X = F32(o + Proto.HsPos);
                s.Y = F32(o + Proto.HsPos + 4);
                s.Z = F32(o + Proto.HsPos + 8);
                s.TeleportSeq = U32(o + Proto.HsTeleportSeq);
                s.ViewportW = U32(o + Proto.HsViewportW);
                s.ViewportH = U32(o + Proto.HsViewportH);
                s.UnitsPerMetre = F32(o + Proto.HsUnitsPerMetre);
                s.Hp = I32(o + Proto.HsHp);
                s.HpMax = I32(o + Proto.HsHpMax);
                s.Yaw = F32(o + Proto.HsYaw);
                s.GameHour = F32(o + Proto.HsGameHour);
                Thread.MemoryBarrier();
                if (Volatile.Read(ref I32(o + Proto.HsSeq)) == s1)
                {
                    return true;
                }
            }
            return false;
        }

        /// ULTRAKILL's HUD settings for Elden Ring's overlay (protocol: EK_OFF_HUD_PREFS)
        public static void WriteHudPrefs(int hudType, int crossHair, int crossHairColor, int crossHairHud, int styleMeter, float bgOpacity)
        {
            if (b == null)
            {
                return;
            }
            long o = Proto.OffHudPrefs;
            I32(o + 4) = hudType;
            I32(o + 8) = crossHair;
            I32(o + 12) = crossHairColor;
            I32(o + 16) = crossHairHud;
            I32(o + 20) = styleMeter;
            F32(o + 24) = bgOpacity;
            Thread.MemoryBarrier();
            U32(o) = 1;
        }

        /// ULTRAKILL's style feed for Elden Ring's overlay (protocol: EK_OFF_STYLE_TEXT)
        public static void WriteStyleText(string text)
        {
            if (b == null)
            {
                return;
            }
            byte[] bytes = System.Text.Encoding.UTF8.GetBytes(text ?? "");
            int len = Math.Min(bytes.Length, 1016);
            for (int i = 0; i < len; i++)
            {
                b[Proto.OffStyleText + 8 + i] = bytes[i];
            }
            U32(Proto.OffStyleText + 4) = (uint)len;
            Thread.MemoryBarrier();
            U32(Proto.OffStyleText) = U32(Proto.OffStyleText) + 1;
        }

        public static void WriteGuestState(in GuestState s)
        {
            if (b == null)
            {
                return;
            }
            long o = Proto.OffGuestState;
            int seq = I32(o + Proto.GsSeq);
            Volatile.Write(ref I32(o + Proto.GsSeq), seq + 1);
            Thread.MemoryBarrier();
            U32(o + Proto.GsFlags) = s.Flags;
            U32(o + Proto.GsTeleportAck) = s.TeleportAck;
            U32(o + Proto.GsEpochAck) = s.EpochAck;
            SetV3(o + Proto.GsPos, s.Pos);
            SetV3(o + Proto.GsVel, s.Vel);
            SetV3(o + Proto.GsEye, s.Eye);
            SetV3(o + Proto.GsFwd, s.Fwd);
            SetV3(o + Proto.GsUp, s.Up);
            F32(o + Proto.GsFov) = s.Fov;
            I32(o + Proto.GsHp) = s.Hp;
            F32(o + Proto.GsHardDamage) = s.HardDamage;
            F32(o + Proto.GsStamina) = s.Stamina;
            U32(o + Proto.GsWeapon) = s.Weapon;
            U32(o + Proto.GsStyleRank) = s.StyleRank;
            U32(o + Proto.GsFrame) = s.Frame;
            Native.QueryPerformanceCounter(out long qpc);
            I64(o + Proto.GsFrameQpc) = qpc;
            Volatile.Write(ref I32(o + Proto.GsSeq), seq + 2);
        }

        public static bool PopInput(out InputEvent e)
        {
            e = default;
            if (b == null)
            {
                return false;
            }
            long ring = Proto.OffInputRing;
            long head = Volatile.Read(ref I64(ring + Proto.RingHead));
            long tail = I64(ring + Proto.RingTail);
            if (tail >= head)
            {
                return false;
            }
            if (head - tail > Proto.InputEntries)
            {
                tail = head - Proto.InputEntries;
            }
            byte* p = b + ring + Proto.RingData + (tail & (Proto.InputEntries - 1)) * 16;
            e.Type = *(ushort*)p;
            e.Code = *(ushort*)(p + 2);
            e.A = *(int*)(p + 4);
            e.B = *(int*)(p + 8);
            e.C = *(int*)(p + 12);
            Volatile.Write(ref I64(ring + Proto.RingTail), tail + 1);
            return true;
        }

        public static void PushEvent(uint type, uint actor, float a, float bArg, float c, float d, uint flags, uint weapon)
        {
            if (b == null)
            {
                return;
            }
            long ring = Proto.OffEventRing;
            long head = I64(ring + Proto.RingHead);
            long tail = Volatile.Read(ref I64(ring + Proto.RingTail));
            if (head - tail >= Proto.EventEntries)
            {
                return;  // the host isn't reading: drop rather than overwrite unread hits
            }
            byte* p = b + ring + Proto.RingData + (head & (Proto.EventEntries - 1)) * Proto.EventBytes;
            *(uint*)p = type;
            *(uint*)(p + 4) = actor;
            *(float*)(p + 8) = a;
            *(float*)(p + 12) = bArg;
            *(float*)(p + 16) = c;
            *(float*)(p + 20) = d;
            *(uint*)(p + 24) = flags;
            *(uint*)(p + 28) = weapon;
            Volatile.Write(ref I64(ring + Proto.RingHead), head + 1);
        }

        private static readonly Actor[] actors = new Actor[Proto.MaxActors];

        public static bool ReadActors(out Actor[] table, out int count)
        {
            table = actors;
            count = 0;
            if (b == null)
            {
                return false;
            }
            long o = Proto.OffActorTable;
            for (int attempt = 0; attempt < 16; attempt++)
            {
                int s1 = Volatile.Read(ref I32(o + Proto.AtSeq));
                if ((s1 & 1) != 0)
                {
                    Thread.SpinWait(8);
                    continue;
                }
                count = Math.Min(I32(o + Proto.AtCount), Proto.MaxActors);
                for (int i = 0; i < count; i++)
                {
                    long r = o + Proto.AtRecords + i * Proto.ActorBytes;
                    ref Actor a = ref actors[i];
                    a.Id = U32(r);
                    a.Flags = U32(r + 4);
                    a.Pos = V3(r + 8);
                    a.Yaw = F32(r + 20);
                    a.Radius = F32(r + 24);
                    a.Height = F32(r + 28);
                    a.HpFrac = F32(r + 32);
                    a.Team = *(ushort*)(b + r + 36);
                    a.Name = null;  // decoded on demand (NameOf) to keep this allocation-free
                }
                Thread.MemoryBarrier();
                if (Volatile.Read(ref I32(o + Proto.AtSeq)) == s1)
                {
                    return true;
                }
            }
            count = 0;
            return false;
        }

        public delegate void CollisionSink(uint type, byte* payload, int bytes);

        // The host blocks on a full ring, so it is drained every frame (up to a budget).
        public static void DrainCollision(CollisionSink sink, long maxBytes)
        {
            if (b == null)
            {
                return;
            }
            long ring = Proto.OffCollision;
            long head = Volatile.Read(ref I64(ring + Proto.RingHead));
            long tail = I64(ring + Proto.RingTail);
            byte* data = b + ring + Proto.RingData;
            long size = Proto.ColDataBytes;
            long done = 0;
            while (tail < head && done < maxBytes)
            {
                long pos = tail % size;
                uint type = *(uint*)(data + pos);
                int payload = *(int*)(data + pos + 4);
                if (type == Proto.ColPad)
                {
                    tail += size - pos;
                    continue;
                }
                sink(type, data + pos + 8, payload);
                long msgBytes = (8 + payload + 7) & ~7L;
                tail += msgBytes;
                done += msgBytes;
            }
            Volatile.Write(ref I64(ring + Proto.RingTail), tail);
        }

        public static long CollisionBacklog() => b == null ? 0 : Volatile.Read(ref I64(Proto.OffCollision + Proto.RingHead)) - I64(Proto.OffCollision + Proto.RingTail);

        // Where the next overlay frame goes (the guest's back slot).
        public static byte* OverlayBackPixels => b + Proto.OffOverlayPixels + Proto.OverlaySlotBytes * overlayBack;

        // Publishes the back slot (filled through OverlayBackPixels) and takes the old middle one.
        public static void PublishOverlay(int width, int height, uint flags, ulong frameId)
        {
            if (b == null)
            {
                return;
            }
            byte* h = b + Proto.OffOverlayHdr + Proto.SlotHdrBytes * overlayBack;
            *(uint*)(h + Proto.ShWidth) = (uint)width;
            *(uint*)(h + Proto.ShHeight) = (uint)height;
            *(uint*)(h + Proto.ShFlags) = flags;
            *(ulong*)(h + Proto.ShFrameId) = frameId;
            Thread.MemoryBarrier();
            int old = Interlocked.Exchange(ref I32(Proto.OffOverlayCtl + Proto.OcState), overlayBack | Proto.OverlayDirty);
            overlayBack = old & 3;
            Interlocked.Increment(ref I64(Proto.OffOverlayCtl + Proto.OcFrames));
        }
    }

    internal static class Native
    {
        public const uint FileMapAllAccess = 0xF001F;

        [DllImport("kernel32", SetLastError = true, CharSet = CharSet.Unicode)]
        public static extern IntPtr OpenFileMappingW(uint access, bool inherit, string name);

        [DllImport("kernel32", SetLastError = true)]
        public static extern IntPtr MapViewOfFile(IntPtr mapping, uint access, uint offsetHigh, uint offsetLow, UIntPtr bytes);

        [DllImport("kernel32")]
        public static extern bool UnmapViewOfFile(IntPtr view);

        [DllImport("kernel32")]
        public static extern bool CloseHandle(IntPtr handle);

        [DllImport("kernel32", CharSet = CharSet.Unicode)]
        public static extern IntPtr CreateMutexW(IntPtr attributes, bool initialOwner, string name);

        [DllImport("kernel32")]
        public static extern ulong GetTickCount64();

        [DllImport("kernel32")]
        public static extern IntPtr OpenProcess(uint access, bool inherit, uint pid);

        [DllImport("kernel32")]
        public static extern bool GetExitCodeProcess(IntPtr process, out uint code);

        // Whether a process still runs (exit code STILL_ACTIVE).
        public static bool ProcessAlive(uint pid)
        {
            if (pid == 0)
            {
                return false;
            }
            IntPtr h = OpenProcess(0x1000 /* PROCESS_QUERY_LIMITED_INFORMATION */, false, pid);
            if (h == IntPtr.Zero)
            {
                return false;
            }
            bool alive = GetExitCodeProcess(h, out uint code) && code == 259;
            CloseHandle(h);
            return alive;
        }

        [DllImport("kernel32")]
        public static extern uint GetCurrentProcessId();

        [DllImport("kernel32")]
        public static extern bool QueryPerformanceCounter(out long value);

        // ---- the hidden window ----
        public const int GwlExStyle = -20;
        public const long WsExToolWindow = 0x80, WsExAppWindow = 0x40000;
        public const uint SwpNoSize = 0x1, SwpNoZOrder = 0x4, SwpNoActivate = 0x10, SwpFrameChanged = 0x20;

        [DllImport("user32")]
        public static extern IntPtr GetActiveWindow();

        [DllImport("user32", CharSet = CharSet.Unicode)]
        public static extern IntPtr FindWindowW(string className, string title);

        [DllImport("user32")]
        public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);

        [DllImport("user32", EntryPoint = "GetWindowLongPtrW")]
        public static extern long GetWindowLongPtr(IntPtr hwnd, int index);

        [DllImport("user32", EntryPoint = "SetWindowLongPtrW")]
        public static extern long SetWindowLongPtr(IntPtr hwnd, int index, long value);

        [DllImport("user32")]
        public static extern bool SetWindowPos(IntPtr hwnd, IntPtr after, int x, int y, int cx, int cy, uint flags);

        public delegate bool EnumWindowsProc(IntPtr hwnd, IntPtr param);

        [DllImport("user32")]
        public static extern bool EnumWindows(EnumWindowsProc proc, IntPtr param);

        [DllImport("user32")]
        public static extern bool IsWindowVisible(IntPtr hwnd);
    }
}
