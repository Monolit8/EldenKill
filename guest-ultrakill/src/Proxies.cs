using System.Collections.Generic;
using HarmonyLib;
using UnityEngine;

namespace EldenKill
{
    // Elden Ring's enemies, as hitboxes V1's weapons can hit. Each is a capsule on ULTRAKILL's
    // enemy-limb layer with an EnemyIdentifier, which is what revolver beams, shotgun pellets,
    // punches, rockets and explosions all look for; that EnemyIdentifier never runs (its Unity
    // messages are skipped below) and the damage it would take goes to Elden Ring instead, as a
    // HIT_ACTOR event. Elden Ring keeps the real enemy: its AI, animation, death and runes.
    // (Killcraft's Combat.cs mirrors enemies the same way, for Minecraft's sword.)
    internal static class Proxies
    {
        public const int LimbLayer = 10;

        private static readonly Dictionary<uint, Proxy> live = new Dictionary<uint, Proxy>();
        private static readonly HashSet<uint> seen = new HashSet<uint>();
        private static readonly Dictionary<uint, (Vector3 at, float size, float t)> recent = new Dictionary<uint, (Vector3, float, float)>();
        private static GameObject root;

        public static int Count => live.Count;

        /// Debug (Num2 in Elden Ring): the hitboxes drawn as red capsules in V1's view.
        public static bool ShowDebug;
        private static Material hitboxMaterial;

        public static void Frame(bool active)
        {
            if (!active || !Link.ReadActors(out Actor[] table, out int count))
            {
                if (!active)
                {
                    Clear();
                }
                return;
            }
            if (root == null)
            {
                root = new GameObject("EldenKill enemies");
                Object.DontDestroyOnLoad(root);
                live.Clear();
            }
            seen.Clear();
            for (int i = 0; i < count; i++)
            {
                ref Actor a = ref table[i];
                if ((a.Flags & Proto.ActorDead) != 0)
                {
                    continue;
                }
                seen.Add(a.Id);
                if (!live.TryGetValue(a.Id, out Proxy p) || p == null)
                {
                    p = Create(a.Id);
                    live[a.Id] = p;
                }
                p.Place(a);
            }
            if (live.Count != seen.Count)
            {
                var gone = new List<uint>();
                foreach (var kv in live)
                {
                    if (!seen.Contains(kv.Key))
                    {
                        gone.Add(kv.Key);
                    }
                }
                foreach (uint id in gone)
                {
                    if (live[id] != null)
                    {
                        // remembered a moment: a kill arrives after the dead enemy left the table
                        var lp = live[id];
                        recent[id] = (lp.transform.position + Vector3.up * lp.Capsule.height * 0.5f, lp.Capsule.height, Time.time);
                        Object.Destroy(live[id].gameObject);
                    }
                    live.Remove(id);
                }
            }
        }

        // The hitboxes are only for V1's weapons: V1's own body passes through them. They move by
        // being teleported every frame (to Elden Ring's characters), and a hitbox appearing inside
        // V1, or V1 running into one, was answered by the physics engine with a huge shove: V1 was
        // launched through the air for no visible reason.
        private static Collider[] v1Colliders;

        public static Material HitboxMaterial => hitboxMaterial ??= new Material(Shader.Find("Standard") ?? Shader.Find("Unlit/Color"));

        private static void IgnoreV1(Proxy proxy)
        {
            var nm = MonoSingleton.GetInstance(typeof(NewMovement)) as NewMovement;
            if (nm == null)
            {
                return;
            }
            v1Colliders = nm.GetComponentsInChildren<Collider>(true);
            foreach (var c in v1Colliders)
            {
                if (c == null)
                {
                    continue;
                }
                Physics.IgnoreCollision(proxy.Capsule, c, true);
                Physics.IgnoreCollision(proxy.Head, c, true);
            }
        }

        /// Where actor `id`'s hitbox is (its middle), if it's still there.
        public static bool Where(uint id, out Vector3 at, out float size)
        {
            at = default;
            size = 0f;
            if (!live.TryGetValue(id, out Proxy p) || p == null)
            {
                if (recent.TryGetValue(id, out var r) && Time.time - r.t < 3f)
                {
                    at = r.at;
                    size = r.size;
                    return true;
                }
                return false;
            }
            at = p.transform.position + Vector3.up * p.Capsule.height * 0.5f;
            size = p.Capsule.height;
            return true;
        }

        /// The hitbox nearest to p (for the launch log): its id, distance and size.
        public static string Nearest(Vector3 p)
        {
            Proxy best = null;
            float bestD = float.MaxValue;
            foreach (var x in live.Values)
            {
                if (x == null)
                {
                    continue;
                }
                float d = Vector3.Distance(x.transform.position, p);
                if (d < bestD)
                {
                    bestD = d;
                    best = x;
                }
            }
            return best == null ? "no enemy hitboxes" : $"nearest enemy hitbox {best.Id:X8} at {bestD:0.0} units (radius {best.Capsule.radius:0.0}, height {best.Capsule.height:0.0})";
        }

        public static void Clear()
        {
            foreach (var p in live.Values)
            {
                if (p != null)
                {
                    Object.Destroy(p.gameObject);
                }
            }
            live.Clear();
        }

        private static Proxy Create(uint id)
        {
            // weapons only count colliders tagged as enemy parts (RevolverBeam: Enemy/Body/Limb/Head)
            var go = new GameObject($"er-actor-{id:X8}") { layer = LimbLayer, tag = "Body" };
            go.SetActive(false);  // no Awake until it's set up
            go.transform.SetParent(root.transform, false);
            var proxy = go.AddComponent<Proxy>();
            proxy.Id = id;
            proxy.Capsule = go.AddComponent<CapsuleCollider>();
            var rb = go.AddComponent<Rigidbody>();
            rb.isKinematic = true;
            rb.useGravity = false;
            var eid = go.AddComponent<EnemyIdentifier>();
            // a heavy kind: the whiplash pulls V1 to it (Elden Ring's enemy can't be pulled), as
            // with ULTRAKILL's big enemies
            eid.enemyType = EnemyType.Swordsmachine;
            eid.health = 1000000f;
            proxy.Eid = eid;
            go.AddComponent<EnemyIdentifierIdentifier>().eid = eid;
            var head = new GameObject("head") { layer = LimbLayer, tag = "Head" };
            head.transform.SetParent(go.transform, false);
            proxy.Head = head.AddComponent<SphereCollider>();
            head.AddComponent<EnemyIdentifierIdentifier>().eid = eid;
            // the head is the weak point: coin ricochets and the railcannon's aim go for it
            eid.weakPoint = head;
            Patches.ProxyEids.Add(eid);
            go.SetActive(true);
            IgnoreV1(proxy);
            // in ULTRAKILL's enemy list: coins, auto-aim and anything else that looks for enemies
            // find Elden Ring's (its own EnemyIdentifier code, which would register it, doesn't run)
            try
            {
                MonoSingleton<EnemyTracker>.Instance?.AddEnemy(eid);
            }
            catch (System.Exception e)
            {
                Plugin.Log.LogWarning($"enemy tracker: {e.Message}");
            }
            return proxy;
        }
    }

    internal sealed class Proxy : MonoBehaviour
    {
        public uint Id;
        public CapsuleCollider Capsule;
        public SphereCollider Head;
        public EnemyIdentifier Eid;
        private GameObject shape;

        public void Place(in Actor a)
        {
            // Elden Ring's radius is its movement capsule, slimmer than the body you see: a fifth
            // more, forgiving like ULTRAKILL's own hitboxes
            float r = Mathf.Max(0.35f, a.Radius * 1.2f), h = Mathf.Max(r * 2f, a.Height);
            Capsule.radius = r;
            Capsule.height = h;
            Capsule.center = new Vector3(0f, h * 0.5f, 0f);
            // the head (crits): a bit forward of the top, a quarter of the body wide, proud of the
            // capsule so rays reach it first
            float hr = Mathf.Clamp(r * 0.6f, 0.3f, h * 0.14f);
            Head.radius = hr;
            Head.center = new Vector3(0f, h * 0.9f - hr * 0.5f, r * 0.35f);
            if (Proxies.ShowDebug && shape == null)
            {
                shape = GameObject.CreatePrimitive(PrimitiveType.Capsule);
                Object.Destroy(shape.GetComponent<Collider>());
                shape.transform.SetParent(transform, false);
                Proxies.HitboxMaterial.color = new Color(1f, 0.15f, 0.1f);
                shape.GetComponent<MeshRenderer>().sharedMaterial = Proxies.HitboxMaterial;
            }
            if (shape != null)
            {
                shape.SetActive(Proxies.ShowDebug);
                // Unity's capsule is 2 tall and 1 wide at scale 1
                shape.transform.localScale = new Vector3(r * 2f, h * 0.5f, r * 2f);
                shape.transform.localPosition = new Vector3(0f, h * 0.5f, 0f);
            }
            transform.SetPositionAndRotation(a.Pos, Quaternion.Euler(0f, a.Yaw, 0f));
        }

        private void OnDestroy()
        {
            if (Eid != null)
            {
                Eid.dead = true;  // out of ULTRAKILL's enemy list
            }
            Patches.ProxyEids.Remove(Eid);
        }
    }

    [HarmonyPatch]
    internal static class Patches
    {
        public static readonly HashSet<EnemyIdentifier> ProxyEids = new HashSet<EnemyIdentifier>();

        // Set by Guest: while linked, Elden Ring decides about deaths and respawns.
        public static bool Linked;

        // A proxy's EnemyIdentifier is only a damage receiver: none of its own logic runs.
        [HarmonyPrefix]
        [HarmonyPatch(typeof(EnemyIdentifier), "Awake")]
        [HarmonyPatch(typeof(EnemyIdentifier), "OnEnable")]
        [HarmonyPatch(typeof(EnemyIdentifier), "OnDisable")]
        [HarmonyPatch(typeof(EnemyIdentifier), "Start")]
        [HarmonyPatch(typeof(EnemyIdentifier), "Update")]
        private static bool SkipOnProxy(EnemyIdentifier __instance) => !ProxyEids.Contains(__instance);

        [HarmonyPrefix]
        [HarmonyPatch(typeof(EnemyIdentifier), nameof(EnemyIdentifier.DeliverDamage))]
        private static bool DamageToEldenRing(EnemyIdentifier __instance, GameObject target, Vector3 hitPoint, float multiplier, float critMultiplier, bool fromExplosion)
        {
            if (!ProxyEids.Contains(__instance))
            {
                return true;
            }
            var proxy = __instance.GetComponent<Proxy>();
            if (proxy != null)
            {
                uint flags = fromExplosion ? Proto.HitExplosion : Proto.HitProjectile;
                bool head = target != null && target.CompareTag("Head");
                if (head)
                {
                    flags |= Proto.HitHeadshot;
                }
                // ULTRAKILL's own rule: a head hit adds critMultiplier times the damage
                float damage = multiplier * (head ? 1f + Mathf.Max(0f, critMultiplier) : 1f) * Plugin.DamageToEldenRing.Value;
                string hitter = __instance.hitter ?? "";
                if (hitter == "punch" || hitter == "heavypunch" || hitter == "ground slam")
                {
                    flags = (flags & ~Proto.HitProjectile) | Proto.HitMelee;
                }
                Link.PushEvent(Proto.EvHitActor, proxy.Id, damage, hitPoint.x, hitPoint.y, hitPoint.z, flags, (uint)hitter.GetHashCode());
                HitFeedback(__instance, target, hitPoint, head, fromExplosion, multiplier);
                if (Plugin.Diagnostics.Value)
                {
                    Plugin.Log.LogInfo($"hit Elden Ring actor {proxy.Id:X8} with {hitter} for {damage:0.00}{(head ? " (head)" : "")}");
                }
            }
            return false;
        }

        // While linked, V1 has every weapon (the save isn't touched: this only answers ULTRAKILL's
        // "is this unlocked?" question for the session). A fresh save has none, and the Sandbox
        // arena would hand V1 only the spawner arm.
        [HarmonyPostfix]
        [HarmonyPatch(typeof(GameProgressSaver), nameof(GameProgressSaver.CheckGear))]
        private static void AllGearWhileLinked(ref int __result)
        {
            if ((Linked || Plugin.Practice) && Plugin.UnlockWeapons.Value && __result < 1)
            {
                __result = 1;
            }
        }

        // The Sandbox arena's "CHEATS ENABLED" box and spawner-arm hints mean nothing in Elden Ring.
        // (CheatsController switches them on in its Update, so they're switched off after it.)
        [HarmonyPostfix]
        [HarmonyPatch(typeof(CheatsController), nameof(CheatsController.Update))]
        private static void NoCheatsPanels(CheatsController __instance)
        {
            if (!Linked)
            {
                return;
            }
            Traverse.Create(__instance).Field<GameObject>("cheatsEnabledPanel").Value?.SetActive(false);
            Traverse.Create(__instance).Field<GameObject>("cheatsInfoPanel").Value?.SetActive(false);
        }

        // ULTRAKILL's feel on Elden Ring's enemies: its blood at the hit (drawn in V1's view, and it
        // heals V1 up close as in ULTRAKILL) and a short hit-freeze on head hits and heavy blows.
        // (The proxies skip the enemy code that does this in ULTRAKILL.)
        private static void HitFeedback(EnemyIdentifier eid, GameObject target, Vector3 hitPoint, bool head, bool fromExplosion, float multiplier)
        {
            try
            {
                var bsm = MonoSingleton<BloodsplatterManager>.Instance;
                if (bsm != null && eid.hitter != "fire")
                {
                    var type = head ? GoreType.Head : multiplier >= 1f || fromExplosion ? GoreType.Body : multiplier >= 0.5f ? GoreType.Small : GoreType.Smallest;
                    var gore = bsm.GetGore(type, eid, fromExplosion);
                    if (gore != null)
                    {
                        gore.transform.position = hitPoint;
                        gore.SetActive(true);
                        var splat = gore.GetComponent<Bloodsplatter>();
                        if (splat != null)
                        {
                            splat.GetReady();
                        }
                    }
                }
                var style = MonoSingleton<StyleHUD>.Instance;
                if (style != null)
                {
                    if (head)
                    {
                        style.AddPoints(20, "ultrakill.headshot");
                    }
                    else if (fromExplosion)
                    {
                        style.AddPoints(10, "ultrakill.explosionhit");
                    }
                }
                var time = MonoSingleton<TimeController>.Instance;
                if (time != null && (head || multiplier >= 2f))
                {
                    time.HitStop(head ? 0.06f : 0.04f);
                }
            }
            catch (System.Exception e)
            {
                Plugin.Log.LogWarning($"hit feedback: {e.Message}");
            }
        }

        // ULTRAKILL's own launches (jumps, wall jumps, explosions, knockback): Guest.cs lets those
        // through and stops upward kicks that come from nowhere (collision arriving around V1).
        internal static float LastLaunch = -99f;

        [HarmonyPrefix]
        [HarmonyPatch(typeof(NewMovement), nameof(NewMovement.Jump))]
        private static void NoteJump() => LastLaunch = Time.time;

        [HarmonyPrefix]
        [HarmonyPatch(typeof(NewMovement), "WallJump")]
        private static void NoteWallJump() => LastLaunch = Time.time;

        [HarmonyPrefix]
        [HarmonyPatch(typeof(NewMovement), nameof(NewMovement.LaunchUp))]
        private static void NoteLaunchUp() => LastLaunch = Time.time;

        [HarmonyPrefix]
        [HarmonyPatch(typeof(NewMovement), nameof(NewMovement.Launch))]
        private static void NoteLaunch() => LastLaunch = Time.time;

        [HarmonyPrefix]
        [HarmonyPatch(typeof(NewMovement), nameof(NewMovement.LaunchFromPoint))]
        private static void NoteLaunchFromPoint() => LastLaunch = Time.time;

        [HarmonyPrefix]
        [HarmonyPatch(typeof(NewMovement), nameof(NewMovement.LaunchFromPointAtSpeed))]
        private static void NoteLaunchAtSpeed() => LastLaunch = Time.time;

        // ULTRAKILL keeps the last camera / input lock when no state asks for one any more: the
        // Sandbox spawn menu locks the camera, Guest.cs hides that menu, its state goes away, and
        // the camera stayed locked (V1 walked but the mouse turned nothing). With no state asking,
        // nothing is locked.
        [HarmonyPostfix]
        [HarmonyPatch(typeof(GameStateManager), "EvaluateState")]
        private static void UnlockWhenNothingLocks(GameStateManager __instance)
        {
            var states = Traverse.Create(__instance).Field<Dictionary<string, GameState>>("activeStates").Value;
            if (states == null)
            {
                return;
            }
            bool camera = false, input = false;
            foreach (var st in states.Values)
            {
                camera |= st.cameraInputLock != LockMode.None;
                input |= st.playerInputLock != LockMode.None;
            }
            if (!camera && __instance.CameraLocked)
            {
                AccessTools.PropertySetter(typeof(GameStateManager), nameof(GameStateManager.CameraLocked)).Invoke(__instance, new object[] { false });
            }
            if (!input && __instance.PlayerInputLocked)
            {
                AccessTools.PropertySetter(typeof(GameStateManager), nameof(GameStateManager.PlayerInputLocked)).Invoke(__instance, new object[] { false });
            }
        }

        // ULTRAKILL restarting the scene after V1 dies would throw the Lands Between away; while
        // linked, Elden Ring's own death and respawn bring V1 back (Guest.cs).
        [HarmonyPrefix]
        [HarmonyPatch(typeof(StatsManager), nameof(StatsManager.Restart))]
        private static bool NoRestartWhileLinked() => !Linked;
    }
}
