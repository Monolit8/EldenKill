using System.Linq;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.InputSystem;
using UnityEngine.InputSystem.LowLevel;

namespace EldenKill
{
    // Elden Ring's keyboard and mouse, fed to ULTRAKILL's Input System as if they were its own
    // devices. ULTRAKILL's window never has the focus (it's hidden), so the real devices send it
    // nothing; the host captures them (DirectInput) and sends them over the input ring.
    // Queued in onBeforeUpdate, so they're applied in the same frame they're read.
    internal static class InputInject
    {
        private static readonly HashSet<Key> keys = new HashSet<Key>();
        private static Vector2 mouseDelta;
        private static float scroll;
        private static int buttons;  // bit per MouseButton
        private static bool keysDirty, mouseDirty, installed;

        // Set by Guest every frame: whether V1 takes input now (else everything is released).
        public static bool Enabled;
        private static Vector2 pointer;
        private static bool havePointer;
        /// mouse look queued to the virtual mouse (diagnostics)
        public static float DeltaQueued;
        // Hurt events from the input ring, handled by Guest on the main thread.
        public static readonly Queue<InputEvent> Hurts = new Queue<InputEvent>();
        /// Elden Ring enemies V1 killed (a = 1 for a boss): style points
        public static readonly Queue<InputEvent> Kills = new Queue<InputEvent>();

        public static void Install()
        {
            if (installed)
            {
                return;
            }
            installed = true;
            // Keep reading devices while unfocused: the window is never focused.
            InputSystem.settings.backgroundBehavior = InputSettings.BackgroundBehavior.IgnoreFocus;
            InputSystem.onBeforeUpdate += BeforeUpdate;
        }

        // Started by Elden Ring, ULTRAKILL gets its keyboard and mouse only from the link: its own
        // virtual devices, and the real ones switched off (in the background Unity still reads
        // them, so every key arrived twice and V1 fired while Elden Ring's menus were open).
        private static Keyboard linkKeyboard;
        private static Mouse linkMouse;

        private static void TakeOverDevices()
        {
            if (linkKeyboard != null || !Plugin.LaunchedHidden)
            {
                return;
            }
            foreach (var d in InputSystem.devices.ToArray())
            {
                if ((d is Keyboard || d is Mouse) && d.native && d.enabled)
                {
                    InputSystem.DisableDevice(d);
                }
            }
            linkKeyboard = InputSystem.AddDevice<Keyboard>("EldenKill Keyboard");
            linkMouse = InputSystem.AddDevice<Mouse>("EldenKill Mouse");
            linkKeyboard.MakeCurrent();
            linkMouse.MakeCurrent();
            Plugin.Log.LogInfo("input: real keyboard and mouse off; V1's input comes from Elden Ring only");
        }

        private static void BeforeUpdate()
        {
            if (!Link.Ready)
            {
                return;
            }
            TakeOverDevices();
            while (Link.PopInput(out InputEvent e))
            {
                switch (e.Type)
                {
                    case Proto.InKey:
                    {
                        Key k = VkToKey(e.Code);
                        if (k != Key.None && (e.A != 0 ? keys.Add(k) : keys.Remove(k)))
                        {
                            keysDirty = true;
                        }
                        break;
                    }
                    case Proto.InMouseButton:
                    {
                        int bit = 1 << e.Code;
                        int now = e.A != 0 ? buttons | bit : buttons & ~bit;
                        mouseDirty |= now != buttons;
                        buttons = now;
                        break;
                    }
                    case Proto.InMouseMove:
                        // DirectInput counts are raw mickeys, which is what Unity's Mouse.delta is too
                        mouseDelta += new Vector2(e.A, -e.B);
                        mouseDirty = true;
                        break;
                    case Proto.InScroll:
                        scroll += e.A;
                        mouseDirty = true;
                        break;
                    case Proto.InMousePos:
                        // ULTRAKILL's menus: where the cursor is on Elden Ring's picture (0-10000 each
                        // way, from the top left)
                        pointer = new Vector2(e.A / 10000f * Screen.width, (1f - e.B / 10000f) * Screen.height);
                        havePointer = true;
                        mouseDirty = true;
                        break;
                    case Proto.InRelease:
                        ReleaseAll();
                        break;
                    case Proto.InHurt:
                        Hurts.Enqueue(e);
                        break;
                    case Proto.InKilled:
                        Kills.Enqueue(e);
                        break;
                }
            }
            if (!Enabled && (keys.Count > 0 || buttons != 0))
            {
                ReleaseAll();
            }
            if (!Enabled)
            {
                mouseDelta = Vector2.zero;
                scroll = 0f;
            }
            var keyboard = linkKeyboard ?? Keyboard.current;
            var mouse = linkMouse ?? Mouse.current;
            // A window that starts unfocused gets its devices disabled before IgnoreFocus is set,
            // and a disabled device drops every event queued to it.
            if (keyboard != null && !keyboard.enabled)
            {
                InputSystem.EnableDevice(keyboard);
                Plugin.Log.LogInfo("input: re-enabled the keyboard (disabled while unfocused)");
            }
            if (mouse != null && !mouse.enabled)
            {
                InputSystem.EnableDevice(mouse);
                Plugin.Log.LogInfo("input: re-enabled the mouse (disabled while unfocused)");
            }
            if (keysDirty && keyboard != null)
            {
                var array = new Key[keys.Count];
                keys.CopyTo(array);
                InputSystem.QueueStateEvent(keyboard, new KeyboardState(array));
                keysDirty = false;
                if (Plugin.Diagnostics.Value)
                {
                    Plugin.Log.LogInfo($"input: keys now [{string.Join(", ", array)}]");
                }
            }
            if (mouseDirty && mouse != null)
            {
                var state = new MouseState
                {
                    position = havePointer ? pointer : mouse.position.ReadValue(),
                    delta = mouseDelta,
                    scroll = new Vector2(0f, scroll),
                };
                for (int i = 0; i < 5; i++)
                {
                    state = state.WithButton((MouseButton)i, (buttons & (1 << i)) != 0);
                }
                InputSystem.QueueStateEvent(mouse, state);
                DeltaQueued += Mathf.Abs(mouseDelta.x) + Mathf.Abs(mouseDelta.y);
                mouseDelta = Vector2.zero;
                scroll = 0f;
                mouseDirty = false;
            }
        }

        public static void ReleaseAll()
        {
            if (keys.Count > 0)
            {
                keys.Clear();
                keysDirty = true;
            }
            if (buttons != 0)
            {
                buttons = 0;
                mouseDirty = true;
            }
        }

        // Windows virtual-key codes -> Input System keys (US layout positions, which is what
        // ULTRAKILL's default bindings mean).
        public static Key VkToKey(int vk)
        {
            if (vk >= 0x41 && vk <= 0x5A)
            {
                return Key.A + (vk - 0x41);
            }
            if (vk >= 0x31 && vk <= 0x39)
            {
                return Key.Digit1 + (vk - 0x31);
            }
            if (vk >= 0x70 && vk <= 0x7B)
            {
                return Key.F1 + (vk - 0x70);
            }
            if (vk >= 0x61 && vk <= 0x69)
            {
                return Key.Numpad1 + (vk - 0x61);
            }
            switch (vk)
            {
                case 0x30: return Key.Digit0;
                case 0x60: return Key.Numpad0;
                case 0x08: return Key.Backspace;
                case 0x09: return Key.Tab;
                case 0x0D: return Key.Enter;
                case 0x10: case 0xA0: return Key.LeftShift;
                case 0xA1: return Key.RightShift;
                case 0x11: case 0xA2: return Key.LeftCtrl;
                case 0xA3: return Key.RightCtrl;
                case 0x12: case 0xA4: return Key.LeftAlt;
                case 0xA5: return Key.RightAlt;
                case 0x14: return Key.CapsLock;
                case 0x1B: return Key.Escape;
                case 0x20: return Key.Space;
                case 0x21: return Key.PageUp;
                case 0x22: return Key.PageDown;
                case 0x23: return Key.End;
                case 0x24: return Key.Home;
                case 0x25: return Key.LeftArrow;
                case 0x26: return Key.UpArrow;
                case 0x27: return Key.RightArrow;
                case 0x28: return Key.DownArrow;
                case 0x2D: return Key.Insert;
                case 0x2E: return Key.Delete;
                case 0xBA: return Key.Semicolon;
                case 0xBB: return Key.Equals;
                case 0xBC: return Key.Comma;
                case 0xBD: return Key.Minus;
                case 0xBE: return Key.Period;
                case 0xBF: return Key.Slash;
                case 0xC0: return Key.Backquote;
                case 0xDB: return Key.LeftBracket;
                case 0xDC: return Key.Backslash;
                case 0xDD: return Key.RightBracket;
                case 0xDE: return Key.Quote;
                default: return Key.None;
            }
        }
    }
}
