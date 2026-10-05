using System.Collections.Generic;
using System;
using UnityEngine;
using UnityEngine.SceneManagement;

namespace EldenKill
{
    // ULTRAKILL's half of the link, every frame (after ULTRAKILL's own scripts):
    //  - finds Elden Ring's link, loads the arena and switches its level off;
    //  - builds Elden Ring's collision as it streams in;
    //  - puts V1 where Elden Ring says when it moved the player itself (spawn, grace, warp,
    //    respawn) and freezes V1 while Elden Ring has the player (loading, menus, cutscenes);
    //  - otherwise lets ULTRAKILL run V1 and reports where it is and where it looks, so Elden Ring
    //    moves the Tarnished and its camera there (ULTRAKILL is authoritative for movement, as
    //    Minecraft is in Killcraft);
    //  - turns Elden Ring's hits into V1's damage and V1's death into the Tarnished's.
    [DefaultExecutionOrder(30000)]
    internal sealed class Guest : MonoBehaviour
    {
        // V1's capsule: radius 0.5, height 3.5, centre 0.25 above the transform (from Killcraft).
        public const float FeetBelowRoot = 1.5f;

        private HostState host;
        private bool haveHost, hostWasAlive;
        private uint lastHostPid;
        private float openTimer;
        private uint teleportAck;
        private uint epochAck;
        private bool teleportWaiting;
        private float teleportWait;
        private string strippedScene;
        private bool arenaRequested;
        private bool frozen;
        private bool savedKinematic;
        private bool reportedDeath;
        private uint frame;
        private OverlayOut overlay;
        private GUIStyle style;
        private float windowTimer;
        private float hostCheck;

        private static T Find<T>() where T : MonoSingleton<T>
        {
            T found = MonoSingleton.GetInstance(typeof(T)) as T;
            return found != null ? found : null;
        }

        private void Awake()
        {
            overlay = gameObject.AddComponent<OverlayOut>();
            InputInject.Install();
        }

        private bool practiceLoaded, practiceScreen;

        // Practice: from the main menu into the Sandbox once; nothing else of EldenKill runs.
        private void PracticeUpdate()
        {
            // full screen on the main monitor once (the window opened off-screen: the hidden copy
            // EldenKill runs is parked there, and ULTRAKILL remembers where its window was)
            if (!practiceScreen && Time.frameCount > 5)
            {
                practiceScreen = true;
                var r = Screen.mainWindowDisplayInfo;
                var main = Screen.currentResolution;
                Screen.MoveMainWindowTo(new DisplayInfo { width = main.width, height = main.height, name = r.name }, Vector2Int.zero);
                Screen.SetResolution(Display.main.systemWidth, Display.main.systemHeight, FullScreenMode.FullScreenWindow);
            }
            var options = Find<OptionsManager>();
            if (!practiceLoaded && options != null && options.mainMenu && SceneHelper.PendingScene == null)
            {
                practiceLoaded = true;
                Plugin.Log.LogInfo("practice: loading the Sandbox (uk_construct)");
                SceneHelper.LoadScene("uk_construct");
            }
        }

        private void Update()
        {
            if (Plugin.Practice)
            {
                PracticeUpdate();
                return;
            }
            openTimer -= Time.unscaledDeltaTime;
            if (!Link.Ready && openTimer <= 0f)
            {
                openTimer = 1f;
                Link.TryOpen();
            }
            Window.Frame(ref windowTimer);
            // Elden Ring's camera can only move when V1 does: ULTRAKILL at 30 fps made the view
            // step ("wobble"). Hidden, it runs at Plugin.Fps (80, Elden Ring's rate too: no vsync;
            // ULTRAKILL's own settings may set these again, so they're kept here).
            if (Plugin.LaunchedHidden && (QualitySettings.vSyncCount != 0 || Application.targetFrameRate != Plugin.Fps.Value))
            {
                QualitySettings.vSyncCount = 0;
                Application.targetFrameRate = Plugin.Fps.Value;
            }
            if (!Link.Ready)
            {
                Patches.Linked = false;
                return;
            }
            Link.Heartbeat();

            bool alive = Link.HostAlive();
            HostState fresh = default;
            if (alive && Link.ReadHostState(ref fresh))
            {
                host = fresh;
                haveHost = true;
            }
            else if (!alive)
            {
                haveHost = false;
            }
            uint pid = Link.HostPid();
            if (alive && (!hostWasAlive || pid != lastHostPid))
            {
                Plugin.Log.LogInfo($"Elden Ring is linked (pid {pid})");
                lastHostPid = pid;
                teleportWaiting = true;
            }
            else if (!alive && hostWasAlive)
            {
                Plugin.Log.LogWarning("Elden Ring stopped answering");
                if (Plugin.LaunchedHidden && !Native.ProcessAlive(Link.HostPid()))
                {
                    // nothing to show without Elden Ring: a hidden ULTRAKILL would linger forever
                    Plugin.Log.LogInfo("quitting (started by Elden Ring, which is gone)");
                    Application.Quit();
                }
            }
            hostWasAlive = alive;
            // a hidden ULTRAKILL outlives a crashed Elden Ring otherwise
            hostCheck -= Time.unscaledDeltaTime;
            if (!alive && Plugin.LaunchedHidden && hostCheck <= 0f)
            {
                hostCheck = 2f;
                if (!Native.ProcessAlive(Link.HostPid()))
                {
                    Plugin.Log.LogInfo("quitting (Elden Ring's process is gone)");
                    Application.Quit();
                }
            }
            Patches.Linked = alive;

            NewMovement nm = Find<NewMovement>();
            OptionsManager options = Find<OptionsManager>();
            CameraController cc = nm != null ? nm.cc : null;
            bool mainMenu = options != null && options.mainMenu;
            bool loading = SceneHelper.PendingScene != null;
            bool paused = options != null && options.paused;
            string scene = SceneHelper.CurrentScene;
            bool inArena = nm != null && cc != null && !mainMenu && !loading && scene == Plugin.ArenaScene.Value;

            // The arena: loaded once Elden Ring is there; its own level switched off once per load.
            if (alive && !inArena && !loading && Plugin.AutoLoadArena.Value && !arenaRequested && (mainMenu || nm == null || scene != Plugin.ArenaScene.Value))
            {
                arenaRequested = true;
                Plugin.Log.LogInfo($"loading the arena ({Plugin.ArenaScene.Value})");
                SceneHelper.LoadScene(Plugin.ArenaScene.Value);
            }
            if (inArena && strippedScene != scene)
            {
                strippedScene = scene;
                World.StripLevel(SceneManager.GetActiveScene(), nm.transform);
                SetupCamera(cc);
                teleportWaiting = true;
            }
            if (!inArena && !loading)
            {
                strippedScene = inArena ? strippedScene : null;
            }
            if (loading)
            {
                arenaRequested = false;
                strippedScene = null;
            }

            // the Sandbox arena's spawn menu has no place in Elden Ring
            var spawnMenu = Find<SpawnMenu>();
            if (alive && spawnMenu != null && spawnMenu.gameObject.activeSelf)
            {
                spawnMenu.gameObject.SetActive(false);
            }
            // The level start locks the camera ("pit-falling", PlayerActivatorRelay) until the player
            // walks into the level's activator, which StripLevel switches off: V1 is activated by
            // hand, so the lock is lifted by hand too (V1 walked but the mouse turned nothing).
            var gsm = GameStateManager.Instance;
            if (alive && gsm != null && gsm.IsStateActive("pit-falling"))
            {
                gsm.PopState("pit-falling");
                Plugin.Log.LogInfo("camera: the level's start lock lifted");
            }
            // a lock left over with nothing asking for it is cleared (Proxies.UnlockWhenNothingLocks)
            if (alive && gsm != null && (gsm.CameraLocked || gsm.PlayerInputLocked))
            {
                HarmonyLib.Traverse.Create(gsm).Method("EvaluateState").GetValue();
            }

            World.Check();
            World.Drain();
            if (World.ClearSeen)
            {
                // a lost collision set is reported as epoch 0: Elden Ring sends everything again
                epochAck = World.Lost ? 0 : World.Epoch;
            }
            if (World.Lost)
            {
                teleportWaiting = true;
            }

            // Resize to Elden Ring's back buffer, so the overlay lines up pixel for pixel.
            if (haveHost && host.ViewportW > 0 && host.ViewportH > 0)
            {
                float s = Mathf.Clamp(Plugin.OverlayScale.Value, 0.25f, 1f);
                int w = Mathf.Min((int)(host.ViewportW * s), Proto.OverlayMaxW), h = Mathf.Min((int)(host.ViewportH * s), Proto.OverlayMaxH);
                if ((Screen.width != w || Screen.height != h) && Time.frameCount % 30 == 0)
                {
                    Screen.SetResolution(w, h, FullScreenMode.Windowed);
                }
            }

            // V1's guns are heard: ULTRAKILL's own sound effects volume was 0 (this copy's settings);
            // set for the session only (the settings file isn't written)
            var mixer = Find<AudioMixerController>();
            // The volumes are ULTRAKILL's own (its Audio options, F1 in Elden Ring). SoundVolume < 0
            // (the default) leaves them alone; 0-1 sets the effects once per link.
            if (alive && mixer != null && !soundSet && Plugin.SoundVolume.Value >= 0f)
            {
                soundSet = true;
                mixer.SetSFXVolume(Plugin.SoundVolume.Value);
                Plugin.Log.LogInfo($"sound: effects volume {Plugin.SoundVolume.Value:0.##} (SoundVolume in the config)");
            }
            // ULTRAKILL's HUD settings for Elden Ring's overlay (it draws V1's HUD)
            hudPrefsTimer -= Time.unscaledDeltaTime;
            if (alive && hudPrefsTimer <= 0f)
            {
                hudPrefsTimer = 0.5f;
                var prefs = MonoSingleton<PrefsManager>.Instance;
                if (prefs != null)
                {
                    Link.WriteHudPrefs(prefs.GetInt("hudType"), prefs.GetInt("crossHair"), prefs.GetInt("crossHairColor"), prefs.GetInt("crossHairHud"),
                        prefs.GetBool("styleMeter") ? 1 : 0, prefs.GetFloat("hudBackgroundOpacity"));
                }
            }

            World.SetShowDebug(haveHost && (host.Flags & Proto.HostShowCollision) != 0);
            Proxies.ShowDebug = haveHost && (host.Flags & Proto.HostShowHitboxes) != 0;

            bool hostWorld = haveHost && (host.Flags & Proto.HostInWorld) != 0 && (host.Flags & Proto.HostEnabled) != 0;
            bool hostHasPlayer = (host.Flags & (Proto.HostLoading | Proto.HostCutscene)) != 0;
            bool hostMenu = (host.Flags & Proto.HostMenu) != 0;

            // Elden Ring moved the player (or V1 just arrived): V1 goes there once its floor is in.
            if (haveHost && host.TeleportSeq != teleportAck)
            {
                teleportWaiting = true;
            }
            if (inArena && hostWorld && teleportWaiting && !hostHasPlayer && World.Epoch == host.Epoch)
            {
                var target = new Vector3(host.X, host.Y, host.Z);
                teleportWait += Time.unscaledDeltaTime;
                // V1 waits for its floor (Elden Ring keeps the player meanwhile): placed with none,
                // V1 fell through the map and on forever (no ground stood on yet to go back to)
                bool floor = World.FloorUnder(target, 8f);  // the floor the Tarnished stands on, not one far below
                if (!floor && Mathf.Floor(teleportWait / 5f) != Mathf.Floor((teleportWait - Time.unscaledDeltaTime) / 5f))
                {
                    Plugin.Log.LogInfo($"waiting for Elden Ring's floor under the player ({teleportWait:0} s, {World.RegionCount} regions loaded)");
                }
                // (no "place it anyway" after a while: without a floor V1 clipped into the map and fell;
                // Elden Ring keeps the player meanwhile, and the host places the collision again)
                if (floor)
                {
                    if (nm.dead)
                    {
                        Plugin.Log.LogInfo("Elden Ring respawned the player: V1 respawns");
                        nm.Respawn();
                        reportedDeath = false;
                    }
                    PlaceV1(nm, target, host.Yaw);
                    Activate(nm);
                    teleportAck = host.TeleportSeq;
                    teleportWaiting = false;
                    teleportWait = 0f;
                }
            }

            bool driving = inArena && hostWorld && !teleportWaiting && !hostHasPlayer && !nm.dead;
            if (driving)
            {
                KeepAboveGround(nm);
                UnstickFromProps(nm);
                // sliding over Elden Ring's bumps and slope edges threw V1 up at 45-65 u/s (the
                // LAUNCH lines), dashing too (55 up); a slide or dash (no jump) keeps its upward
                // speed small
                if ((nm.sliding || nm.boost) && !nm.jumping && nm.rb.velocity.y > 12f)
                {
                    var v = nm.rb.velocity;
                    nm.rb.velocity = new Vector3(v.x, 12f, v.z);
                }
                // an upward kick out of nowhere (no jump, launch, explosion or grapple: collision
                // arriving around V1 pushing it out; LAUNCH 60 u/s holding nothing) is stopped
                var hook = MonoSingleton<HookArm>.Instance;
                bool hooked = hook != null && hook.state != HookState.Ready;
                if (nm.rb.velocity.y > 25f && lastVelY < 12f && Time.time - Patches.LastLaunch > 0.5f && !nm.jumping && !hooked)
                {
                    var v = nm.rb.velocity;
                    Plugin.Log.LogInfo($"V1 kicked up to {v.y:0} u/s with no jump or launch: stopped");
                    nm.rb.velocity = new Vector3(v.x, Mathf.Max(lastVelY, 0f), v.z);
                }
                lastVelY = nm.rb.velocity.y;
            }
            Freeze(nm, inArena && !(driving && !hostMenu));
            // (while ULTRAKILL's own menu is open too: its options are used from Elden Ring)
            InputInject.Enabled = driving && !hostMenu;
            // started hidden by Elden Ring: silent until V1 is in its arena (the main menu's sounds
            // played over Elden Ring's title screen)
            if (Plugin.LaunchedHidden)
            {
                float want = alive && inArena && hostWorld ? 1f : 0f;
                if (AudioListener.volume != want)
                {
                    AudioListener.volume = want;
                }
            }
            // ULTRAKILL's menu over Elden Ring: shown whole, on a dark background
            bool menuOpaque = paused && inArena;
            if (menuOpaque != OverlayOut.Opaque && cc != null)
            {
                OverlayOut.Opaque = menuOpaque;
                foreach (var cam in cc.GetComponentsInChildren<Camera>(true))
                {
                    if (cam.clearFlags == CameraClearFlags.SolidColor)
                    {
                        cam.backgroundColor = menuOpaque ? new Color(0.04f, 0.04f, 0.05f, 1f) : OverlayOut.KeyColor;
                    }
                }
            }
            overlay.Active = inArena && hostWorld;
            Proxies.Frame(inArena && hostWorld);

            // Elden Ring's hits on the Tarnished are V1's.
            while (InputInject.Hurts.Count > 0)
            {
                InputEvent e = InputInject.Hurts.Dequeue();
                if (driving && e.A > 0)
                {
                    nm.GetHurt(e.A, true, 1f, e.Code == Proto.HurtMagic, false, 0.35f, false);
                }
            }
            // Kills on Elden Ring's enemies count for ULTRAKILL's style (its own enemy code, which
            // awards them, doesn't run for them)
            while (InputInject.Kills.Count > 0)
            {
                InputEvent e = InputInject.Kills.Dequeue();
                var styleHud = Find<StyleHUD>();
                if (driving && styleHud != null)
                {
                    bool boss = e.A != 0;
                    styleHud.AddPoints(boss ? 240 : 50, boss ? "ultrakill.bigkill" : "ultrakill.kill");
                    KillReward(nm, (uint)e.B, boss);
                    if (Time.time - lastKill < 1.5f)
                    {
                        killChain++;
                        styleHud.AddPoints(killChain >= 3 ? 150 : 100, killChain >= 3 ? "ultrakill.multikill" : killChain == 2 ? "ultrakill.triplekill" : "ultrakill.doublekill");
                    }
                    else
                    {
                        killChain = 0;
                    }
                    lastKill = Time.time;
                }
            }
            // the style feed text goes to Elden Ring (its overlay draws it)
            var feedHud = Find<StyleHUD>();
            if (feedHud != null)
            {
                var tmp = HarmonyLib.Traverse.Create(feedHud).Field<TMPro.TMP_Text>("styleInfo").Value;
                string feed = tmp != null && tmp.gameObject.activeInHierarchy ? tmp.text ?? "" : "";
                if (feed != lastFeed)
                {
                    lastFeed = feed;
                    Link.WriteStyleText(feed);
                }
            }
            // Deaths go both ways.
            if (inArena && nm.dead && !reportedDeath)
            {
                reportedDeath = true;
                Plugin.Log.LogInfo("V1 died: so does the Tarnished");
                Link.PushEvent(Proto.EvDied, 0, 0, 0, 0, 0, 0, 0);
            }
            if (inArena && haveHost && (host.Flags & Proto.HostDead) != 0 && !nm.dead)
            {
                Plugin.Log.LogInfo("the Tarnished died: so does V1");
                reportedDeath = true;
                nm.GetHurt(10000, false, 0f, false, true, 1f, true);
            }

            var gs = new GuestState
            {
                Flags = (inArena ? Proto.GuestInLevel : 0) | (driving ? Proto.GuestDriving : 0) | (paused ? Proto.GuestMenu : 0),
                TeleportAck = teleportAck,
                EpochAck = epochAck,
                Frame = ++frame,
            };
            if (inArena)
            {
                gs.Flags |= (nm.gc != null && nm.gc.onGround ? Proto.GuestOnGround : 0) | (nm.dead ? Proto.GuestDead : 0)
                    | (nm.sliding ? Proto.GuestSliding : 0) | (nm.boost ? Proto.GuestDashing : 0) | (nm.jumping ? Proto.GuestJumping : 0)
                    | (driving && noGround ? Proto.GuestNoGround : 0);
                gs.Pos = nm.transform.position - Vector3.up * FeetBelowRoot;
                gs.Vel = nm.rb.velocity;
                Transform cam = cc.cam.transform;
                gs.Eye = cam.position;
                gs.Fwd = cam.forward;
                gs.Up = cam.up;
                gs.Fov = cc.cam.fieldOfView;
                gs.Hp = nm.hp;
                gs.HardDamage = nm.antiHp;
                gs.Stamina = nm.boostCharge;
                var guns = Find<GunControl>();
                gs.Weapon = guns != null ? (uint)guns.currentSlotIndex : 0;
                // style: rank (bits 0-7), meter fill 0-255 (bits 8-15), combo going (bit 16)
                var style = Find<StyleHUD>();
                if (style != null)
                {
                    var tr = HarmonyLib.Traverse.Create(style);
                    float meter = tr.Field<float>("currentMeter").Value;
                    bool combo = tr.Field<bool>("comboActive").Value;
                    int ri = style.rankIndex;
                    float max = Mathf.Max(1f, style.currentRank.maxMeter);
                    uint fill = (uint)Mathf.Clamp(meter / max * 255f, 0f, 255f);
                    gs.StyleRank = (uint)(ri & 0xFF) | (fill << 8) | (combo ? 1u << 16 : 0u);
                }
            }
            Link.WriteGuestState(gs);
            // (the per-key "input diag" lines were replaced by Elden Ring's debug panel, F8)
        }

        private float diagTimer;

        private void InputDiagnostics(NewMovement nm)
        {
            diagTimer -= Time.unscaledDeltaTime;
            var kb = UnityEngine.InputSystem.Keyboard.current;
            if (diagTimer > 0f || kb == null || !kb.anyKey.isPressed)
            {
                return;
            }
            diagTimer = 0.5f;
            var im = Find<InputManager>();
            Vector2 move = im != null ? im.InputSource.Move.ReadValue<Vector2>() : Vector2.zero;
            Plugin.Log.LogInfo($"input diag: W {kb.wKey.isPressed}, kb enabled {kb.enabled}, Move {move}, activated {nm.activated}, " +
                $"kinematic {nm.rb.isKinematic}, focused {Application.isFocused}, timeScale {Time.timeScale}, vel {nm.rb.velocity}, " +
                $"update mode {UnityEngine.InputSystem.InputSystem.settings.updateMode}, background {UnityEngine.InputSystem.InputSystem.settings.backgroundBehavior}");
        }

        private void PlaceV1(NewMovement nm, Vector3 feet, float yaw)
        {
            Vector3 root = feet + Vector3.up * FeetBelowRoot;
            nm.transform.position = root;
            nm.rb.position = root;
            nm.rb.velocity = Vector3.zero;
            if (nm.cc != null)
            {
                nm.cc.rotationY = yaw;
            }
            Plugin.Log.LogInfo($"V1 placed at {feet} (teleport {host.TeleportSeq}, epoch {host.Epoch}, {World.RegionCount} bodies / {World.TriangleCount} triangles loaded)");
        }

        // What the arena's PlayerActivator trigger does when V1 drops into the first room (we
        // teleport past it): V1 and its camera take input, guns, fists and the HUD come up. The
        // level objects it would also switch on are left alone.
        private static void Activate(NewMovement nm)
        {
            if (nm.activated)
            {
                return;
            }
            nm.activated = true;
            nm.cc.activated = true;
            nm.cc.enabled = true;
            try
            {
                Find<GunControl>()?.YesWeapon();
                Find<FistControl>()?.YesFist();
                Find<StatsManager>()?.UnhideShit();
            }
            catch (Exception e)
            {
                Plugin.Log.LogWarning($"activating V1: {e.Message}");
            }
            Plugin.Log.LogInfo("V1 activated");
        }

        private float underTime, groundLog, lastLaunchLog;
        private bool noGround;
        private float lastGroundY;
        private bool haveGroundY;

        // Elden Ring's ground is paper-thin triangles, and V1 is fast (dashes, slams, the whiplash's
        // pull): in game V1 went straight through it and ended up 32-40 m below, walking on caves.
        // So V1's rigidbody uses continuous collision, and if there's Elden Ring ground above V1 and
        // none below for a moment, V1 is put back on top of it.
        // Sliding or dashing at full speed V1 could end up inside a tree, fence or rock (Elden Ring's
        // props are one-sided triangle shells: once past the surface nothing pushes V1 out) and stay
        // stuck there. Inside = most of the rays around V1 hit the back of a surface; stuck = inside
        // and hardly moving although the player wants to (or slides / dashes). Then V1 goes back to
        // the last spot it was free (inside a room it can still walk around, so nothing happens there).
        private Vector3 lastFree;
        private float freeAge = 99f, stuckTime;
        private readonly Queue<(float t, Vector3 p)> recent = new Queue<(float, Vector3)>();

        private void UnstickFromProps(NewMovement nm)
        {
            const int env = 1 << World.EnvironmentLayer;
            Vector3 pos = nm.transform.position;
            Vector3 c = nm.playerCollider != null ? nm.playerCollider.bounds.center : pos;
            bool hitBack = Physics.queriesHitBackfaces;
            Physics.queriesHitBackfaces = true;
            int back = 0;
            for (int i = 0; i < 8; i++)
            {
                Vector3 dir = Quaternion.Euler(0f, i * 45f, 0f) * Vector3.forward;
                if (Physics.Raycast(c, dir, out RaycastHit hit, 8f, env, QueryTriggerInteraction.Ignore) && Vector3.Dot(hit.normal, dir) > 0.1f)
                {
                    back++;
                }
            }
            Physics.queriesHitBackfaces = hitBack;

            float now = Time.time;
            recent.Enqueue((now, pos));
            while (recent.Count > 0 && now - recent.Peek().t > 0.4f)
            {
                recent.Dequeue();
            }
            float moved = recent.Count > 0 ? (pos - recent.Peek().p).magnitude : 99f;

            // sunk: the ground seen from above is over V1's feet (wedged into a tree's roots or a rock)
            Vector3 feet = pos - Vector3.up * FeetBelowRoot;
            // (only ground just over the feet, facing up: a branch or ledge 1.5 m up isn't ground V1
            // sank into, and lifting V1 onto it made it drop back again and again)
            bool sunk = Physics.Raycast(feet + Vector3.up * 1.2f, Vector3.down, out RaycastHit top, 1.2f, env, QueryTriggerInteraction.Ignore)
                && top.point.y > feet.y + 0.25f && top.normal.y > 0.5f;
            bool inside = back >= 5;
            var input = MonoSingleton<InputManager>.Instance;
            bool wants = nm.sliding || nm.boost || (input != null && input.InputSource.Move.ReadValue<Vector2>().sqrMagnitude > 0.1f);
            // jammed: physics keeps V1 fast (19 u/s in the log) but it doesn't get anywhere
            bool jammed = nm.rb.velocity.magnitude > 6f && moved < 0.8f;
            bool stuck = moved < 0.8f && (jammed || (wants && (inside || sunk)));

            freeAge += Time.deltaTime;
            if (!stuck && !inside && !sunk)
            {
                lastFree = pos;
                freeAge = 0f;
            }
            stuckTime = stuck ? stuckTime + Time.deltaTime : 0f;
            if (stuckTime > 0.35f && freeAge < 15f)
            {
                // sunk into the ground: onto the surface above the feet (going back to the last free
                // spot only moved V1 a few centimetres, and it sank again)
                if (sunk)
                {
                    lastFree = top.point + Vector3.up * (FeetBelowRoot + 0.3f);
                }
                else
                {
                    lastFree += Vector3.up * 0.3f;
                }
                Plugin.Log.LogInfo($"V1 stuck at {pos} (inside {back}/8, sunk {sunk}, jammed {jammed}, speed {nm.rb.velocity.magnitude:0}): back to {lastFree}");
                nm.rb.velocity = Vector3.zero;
                nm.rb.position = lastFree;
                nm.transform.position = lastFree;
                stuckTime = 0f;
                recent.Clear();
            }
        }

        // Why V1's camera might not turn: logged every 2 s while V1 plays
        private float lastVelY;
        private bool soundSet;
        private float hudPrefsTimer;
        private float lastKill = -99f;
        private int killChain;
        private string lastFeed = "";
        private float lookDiag;
        private float lookTotal;

        private void LookDiagnostics(CameraController cc)
        {
            var input = MonoSingleton<InputManager>.Instance;
            if (input != null)
            {
                lookTotal += input.InputSource.Look.ReadValue<Vector2>().magnitude;
            }
            lookDiag += Time.unscaledDeltaTime;
            if (lookDiag < 2f)
            {
                return;
            }
            lookDiag = 0f;
            var gsm = GameStateManager.Instance;
            var states = gsm != null ? HarmonyLib.Traverse.Create(gsm).Field<List<string>>("stateOrder").Value : null;
            var opm = MonoSingleton<OptionsManager>.Instance;
            Plugin.Log.LogInfo($"look: queued {InputInject.DeltaQueued:0} px, Look read {lookTotal:0}, cam activated {cc.activated}, camera locked {gsm?.CameraLocked}, input locked {gsm?.PlayerInputLocked}, states [{(states != null ? string.Join(",", states) : "")}], last device {input?.LastButtonDevice?.name}, freeze {cc.gamepadFreezeCount}, sensitivity {opm?.mouseSensitivity}, mouse {UnityEngine.InputSystem.Mouse.current?.name} enabled {UnityEngine.InputSystem.Mouse.current?.enabled}, cc enabled {cc.enabled}");
            lookTotal = 0f;
        }

        // A kill heals: a burst of blood where the enemy died (standing in it heals more, as in
        // ULTRAKILL) and health straight away at any distance (20, a boss 50)
        private static void KillReward(NewMovement nm, uint actor, bool boss)
        {
            // the health first (the blood used to throw before it, and nothing healed)
            if (!nm.dead)
            {
                nm.GetHealth(boss ? 50 : 20, false, false, false);
            }
            try
            {
                if (Proxies.Where(actor, out Vector3 at, out float size))
                {
                    var bsm = MonoSingleton<BloodsplatterManager>.Instance;
                    int bursts = boss ? 5 : 2;
                    for (int i = 0; bsm != null && i < bursts; i++)
                    {
                        // (the overload without an enemy: the other one needs a real enemy)
                        var gore = bsm.GetGore(i == 0 ? GoreType.Head : GoreType.Body, false, false, false);
                        if (gore == null)
                        {
                            continue;
                        }
                        gore.transform.position = at + UnityEngine.Random.insideUnitSphere * size * 0.2f;
                        gore.SetActive(true);
                        gore.GetComponent<Bloodsplatter>()?.GetReady();
                    }
                }
            }
            catch (System.Exception e)
            {
                Plugin.Log.LogWarning($"kill reward: {e.Message}");
            }
        }

        private void KeepAboveGround(NewMovement nm)
        {
            if (nm.rb.collisionDetectionMode != CollisionDetectionMode.ContinuousDynamic && !nm.rb.isKinematic)
            {
                nm.rb.collisionDetectionMode = CollisionDetectionMode.ContinuousDynamic;
                Plugin.Log.LogInfo("V1's rigidbody: continuous collision on");
            }
            const int env = 1 << World.EnvironmentLayer;
            Vector3 feet = nm.transform.position - Vector3.up * FeetBelowRoot;
            bool below = Physics.Raycast(feet + Vector3.up * 0.5f, Vector3.down, out RaycastHit down, 400f, env, QueryTriggerInteraction.Ignore);
            noGround = !below;
            // the ground above, seen from above (its top faces): cast down from high up to just over V1's head
            bool above = Physics.Raycast(feet + Vector3.up * 300f, Vector3.down, out RaycastHit top, 296f, env, QueryTriggerInteraction.Ignore);
            // a launch: what was around V1 when it happened
            float speed = nm.rb.velocity.magnitude;
            if ((speed > 120f || nm.rb.velocity.y > 45f) && Time.unscaledTime - lastLaunchLog > 1.5f)
            {
                lastLaunchLog = Time.unscaledTime;
                Plugin.Log.LogWarning($"LAUNCH: V1 at {feet} speed {speed:0} (up {nm.rb.velocity.y:0}), {Proxies.Nearest(feet)}, " +
                    $"ground below {(below ? (feet.y - down.point.y).ToString("0.0") : "none")}, sliding {nm.sliding}, boost {nm.boost}");
            }
            groundLog -= Time.unscaledDeltaTime;
            if (Plugin.Diagnostics.Value && groundLog <= 0f)
            {
                groundLog = 2f;
                Plugin.Log.LogInfo($"ground: V1 feet {feet}, below {(below ? (feet.y - down.point.y).ToString("0.0") : "none")}, " +
                    $"surface above {(above ? (top.point.y - feet.y).ToString("0.0") : "none")}, vel {nm.rb.velocity.magnitude:0}, {World.RegionCount} regions");
            }
            // under the map: V1 is falling, there's nothing to stand on below, and the walkable surface
            // above is the one V1 last stood on (not higher): indoors there's always a roof or an
            // upper floor above, and the rescue put V1 on it ("teleported in closed areas")
            if (below && feet.y - down.point.y < 0.6f)
            {
                lastGroundY = feet.y;
                haveGroundY = true;
            }
            bool fromThere = haveGroundY && top.point.y <= lastGroundY + 1.5f && top.point.y >= lastGroundY - 40f;
            bool under = above && fromThere && top.normal.y > 0.5f && top.point.y > feet.y + 3f && nm.rb.velocity.y < -10f
                && (!below || feet.y - down.point.y > 60f);
            underTime = under ? underTime + Time.deltaTime : 0f;
            if (underTime > 0.4f)
            {
                underTime = 0f;
                Vector3 to = top.point + Vector3.up * 0.2f;
                Plugin.Log.LogWarning($"V1 fell through Elden Ring's ground at {feet} (last stood at height {lastGroundY:0.0}): back on top at {to}");
                nm.transform.position = to + Vector3.up * FeetBelowRoot;
                nm.rb.position = nm.transform.position;
                nm.rb.velocity = Vector3.zero;
            }
        }

        // While Elden Ring has the player, V1 hangs where it is (no gravity, no input).
        private void Freeze(NewMovement nm, bool freeze)
        {
            if (nm == null || nm.rb == null || freeze == frozen)
            {
                return;
            }
            frozen = freeze;
            if (freeze)
            {
                savedKinematic = nm.rb.isKinematic;
                nm.rb.velocity = Vector3.zero;
                nm.rb.isKinematic = true;
            }
            else
            {
                nm.rb.isKinematic = savedKinematic;
            }
        }

        // Only V1's own things reach the overlay: the background is the key colour.
        private static void SetupCamera(CameraController cc)
        {
            if (cc == null || cc.cam == null)
            {
                return;
            }
            foreach (var cam in cc.GetComponentsInChildren<Camera>(true))
            {
                if (cam.clearFlags == CameraClearFlags.Skybox || cam.clearFlags == CameraClearFlags.SolidColor)
                {
                    cam.clearFlags = CameraClearFlags.SolidColor;
                    cam.backgroundColor = OverlayOut.KeyColor;
                }
            }
            cc.cam.useOcclusionCulling = false;
        }

        private string StatusText()
        {
            if (!Link.Ready)
            {
                return "EldenKill: waiting for Elden Ring (start it with eldenkill.me3)";
            }
            if (!hostWasAlive)
            {
                return "EldenKill: Elden Ring stopped answering";
            }
            if ((host.Flags & Proto.HostInWorld) == 0)
            {
                return "EldenKill: linked, waiting for Elden Ring to load a character";
            }
            if (teleportWaiting)
            {
                return $"EldenKill: receiving the Lands Between ({World.RegionCount} bodies)...";
            }
            if (!Plugin.Diagnostics.Value)
            {
                return null;
            }
            return $"EldenKill: {World.RegionCount} bodies / {World.TriangleCount} tris, {Proxies.Count} enemies, overlay {OverlayOut.FramesSent}";
        }

        private void OnGUI()
        {
            if (Plugin.LaunchedHidden && Link.Ready)
            {
                return;  // nothing of ours in the overlay
            }
            string text = StatusText();
            if (text == null)
            {
                return;
            }
            style ??= new GUIStyle(GUI.skin.label) { fontSize = 18, wordWrap = true };
            var r = new Rect(12, 12, Screen.width - 24, 60);
            GUI.color = Color.black;
            GUI.Label(new Rect(r.x + 1, r.y + 1, r.width, r.height), text, style);
            GUI.color = Color.white;
            GUI.Label(r, text, style);
        }
    }

    // A ULTRAKILL started by Elden Ring keeps its window out of the way: off-screen and out of the
    // taskbar (not minimised: a minimised Unity window stops rendering).
    internal static class Window
    {
        private static IntPtr hwnd;
        private static bool done;

        public static void Frame(ref float timer)
        {
            if (!Plugin.LaunchedHidden || done)
            {
                return;
            }
            timer -= Time.unscaledDeltaTime;
            if (timer > 0f)
            {
                return;
            }
            timer = 0.5f;
            if (hwnd == IntPtr.Zero)
            {
                uint me = Native.GetCurrentProcessId();
                Native.EnumWindows((h, _) =>
                {
                    Native.GetWindowThreadProcessId(h, out uint pid);
                    if (pid == me && Native.IsWindowVisible(h))
                    {
                        hwnd = h;
                        return false;
                    }
                    return true;
                }, IntPtr.Zero);
            }
            if (hwnd == IntPtr.Zero)
            {
                return;
            }
            long ex = Native.GetWindowLongPtr(hwnd, Native.GwlExStyle);
            Native.SetWindowLongPtr(hwnd, Native.GwlExStyle, (ex | Native.WsExToolWindow) & ~Native.WsExAppWindow);
            Native.SetWindowPos(hwnd, IntPtr.Zero, -32000, -32000, 0, 0, Native.SwpNoSize | Native.SwpNoZOrder | Native.SwpNoActivate | Native.SwpFrameChanged);
            done = true;
            Plugin.Log.LogInfo("window moved off-screen (started by Elden Ring)");
        }
    }
}
