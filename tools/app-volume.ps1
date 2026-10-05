# Sets one process's volume in the Windows volume mixer, live, e.g.:
#   tools\app-volume.ps1 ULTRAKILL 0.4
param([string]$Process = "ULTRAKILL", [float]$Volume = 0.4)

Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;

[ComImport, Guid("BCDE0395-E52F-467C-8E3D-C4579291692E")] class MMDeviceEnumerator {}
[Guid("A95664D2-9614-4F35-A746-DE8DB63617E6"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface IMMDeviceEnumerator { int NotImpl1(); int GetDefaultAudioEndpoint(int flow, int role, out IMMDevice dev); }
[Guid("D666063F-1587-4E43-81F1-B948E807363F"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface IMMDevice { int Activate(ref Guid iid, int ctx, IntPtr p, [MarshalAs(UnmanagedType.IUnknown)] out object o); }
[Guid("77AA99A0-1BD6-484F-8BC7-2C654C9A9B6F"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface IAudioSessionManager2 { int NotImpl1(); int NotImpl2(); int GetSessionEnumerator(out IAudioSessionEnumerator e); }
[Guid("E2F5BB11-0570-40CA-ACDD-3AA01277DEE8"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface IAudioSessionEnumerator { int GetCount(out int n); int GetSession(int i, out IAudioSessionControl2 s); }
[Guid("bfb7ff88-7239-4fc9-8fa2-07c950be9c6d"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface IAudioSessionControl2 {
    int a(); int b(); int c(); int d(); int e(); int f(); int g(); int h(); int i();
    int GetSessionIdentifier(out IntPtr s); int GetSessionInstanceIdentifier(out IntPtr s);
    int GetProcessId(out uint pid);
}
[Guid("87CE5498-68D6-44E5-9215-6DA47EF883D8"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface ISimpleAudioVolume { int SetMasterVolume(float v, ref Guid ctx); int GetMasterVolume(out float v); }

public static class AppVolume {
    public static int Set(uint pid, float vol) {
        var en = (IMMDeviceEnumerator)new MMDeviceEnumerator();
        IMMDevice dev; en.GetDefaultAudioEndpoint(0, 1, out dev);
        var iid = typeof(IAudioSessionManager2).GUID; object o;
        dev.Activate(ref iid, 23, IntPtr.Zero, out o);
        IAudioSessionEnumerator se; ((IAudioSessionManager2)o).GetSessionEnumerator(out se);
        int n, done = 0; se.GetCount(out n);
        for (int i = 0; i < n; i++) {
            IAudioSessionControl2 s; se.GetSession(i, out s);
            uint p; s.GetProcessId(out p);
            if (p != pid) continue;
            var g = Guid.Empty; ((ISimpleAudioVolume)s).SetMasterVolume(vol, ref g); done++;
        }
        return done;
    }
}
"@

foreach ($p in Get-Process $Process -ErrorAction Stop) {
    $n = [AppVolume]::Set([uint32]$p.Id, $Volume)
    "$Process ($($p.Id)): volume $Volume, $n audio session(s)"
}
