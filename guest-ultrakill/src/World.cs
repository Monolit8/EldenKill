using System.Collections.Generic;
using UnityEngine;
using UnityEngine.SceneManagement;

namespace EldenKill
{
    // Elden Ring's collision in ULTRAKILL: every Havok body the host streams becomes one
    // MeshCollider on ULTRAKILL's Environment layer, so V1's ground checks, wall jumps, slides and
    // every gun's raycasts hit the Lands Between. (Killcraft's Collision.cs the other way round.)
    internal static unsafe class World
    {
        public const int EnvironmentLayer = 8;

        private static GameObject root;
        private static readonly Dictionary<ulong, GameObject> regions = new Dictionary<ulong, GameObject>();
        private static readonly Dictionary<ulong, int> regionTris = new Dictionary<ulong, int>();
        private static Material debugMaterial;

        public static uint Epoch { get; private set; }

        /// Debug (Num1 in Elden Ring): Elden Ring's collision drawn grey in V1's view, so it can be
        /// compared with the world it should sit on.
        public static bool ShowDebug { get; private set; }

        public static void SetShowDebug(bool on)
        {
            if (on == ShowDebug)
            {
                return;
            }
            ShowDebug = on;
            foreach (var go in regions.Values)
            {
                if (go == null)
                {
                    continue;
                }
                var mr = go.GetComponent<MeshRenderer>();
                if (on && mr == null)
                {
                    var mesh = go.GetComponent<MeshCollider>().sharedMesh;
                    mesh.RecalculateNormals();
                    go.AddComponent<MeshFilter>().sharedMesh = mesh;
                    mr = go.AddComponent<MeshRenderer>();
                    mr.sharedMaterial = DebugMaterial;
                }
                if (mr != null)
                {
                    mr.enabled = on || Plugin.ShowLandsBetween.Value;
                }
            }
            Plugin.Log.LogInfo($"debug: collision drawing {(on ? "on" : "off")}");
        }

        private static Material DebugMaterial => debugMaterial ??= new Material(Shader.Find("Standard") ?? Shader.Find("Unlit/Color")) { color = new Color(0.5f, 0.5f, 0.55f) };
        public static bool ClearSeen { get; private set; }
        public static int RegionCount => regions.Count;
        public static int TriangleCount { get; private set; }
        public static int FlippedBodies { get; private set; }

        /// The collision was lost (ULTRAKILL destroyed it on a scene reload): Elden Ring must send it again.
        public static bool Lost { get; private set; }

        /// Call every frame: notices the collision being destroyed under us.
        public static void Check()
        {
            if (ClearSeen && root == null && regions.Count > 0)
            {
                Plugin.Log.LogWarning($"collision: ULTRAKILL destroyed it ({regions.Count} regions); asking Elden Ring for it again");
                regions.Clear();
                regionTris.Clear();
                TriangleCount = 0;
                Lost = true;
            }
        }

        private static GameObject Root
        {
            get
            {
                if (root == null)
                {
                    root = new GameObject("EldenKill Lands Between");
                    Object.DontDestroyOnLoad(root);
                    regions.Clear();
                    regionTris.Clear();
                    TriangleCount = 0;
                }
                return root;
            }
        }

        public static void Drain()
        {
            // Big areas arrive as a burst; 8 MB a frame builds them in a few frames without a hitch
            Link.DrainCollision(Handle, 8L << 20);
        }

        private static void Handle(uint type, byte* p, int bytes)
        {
            switch (type)
            {
                case Proto.ColClear:
                    Epoch = *(uint*)p;
                    ClearSeen = true;
                    Lost = false;
                    foreach (var go in regions.Values)
                    {
                        Object.Destroy(go);
                    }
                    regions.Clear();
                    regionTris.Clear();
                    TriangleCount = 0;
                    if (Plugin.Diagnostics.Value)
                    {
                        Plugin.Log.LogInfo($"collision: clear (epoch {Epoch})");
                    }
                    break;
                case Proto.ColRegion:
                {
                    ulong id = *(ulong*)p;
                    uint epoch = *(uint*)(p + 8);
                    int count = *(int*)(p + 12);
                    if (epoch != Epoch || count <= 0 || 16 + (long)count * Proto.ColTriBytes > bytes)
                    {
                        break;  // from an epoch that's gone
                    }
                    Remove(id);
                    regions[id] = Build(id, p + 16, count);
                    regionTris[id] = count;
                    TriangleCount += count;
                    break;
                }
                case Proto.ColRemove:
                    Remove(*(ulong*)p);
                    break;
            }
        }

        private static void Remove(ulong id)
        {
            if (regions.TryGetValue(id, out var old))
            {
                TriangleCount -= regionTris.TryGetValue(id, out int n) ? n : 0;
                regionTris.Remove(id);
                Object.Destroy(old.GetComponent<MeshCollider>().sharedMesh);
                Object.Destroy(old);
                regions.Remove(id);
            }
        }

        private static GameObject Build(ulong id, byte* tris, int count)
        {
            var vertices = new Vector3[count * 3];
            var indices = new int[count * 3];
            // Which way round are the faces? A MeshCollider is only solid from the front, and Havok's
            // winding isn't known (in game, cells flipped as a whole let V1 dash through floors and
            // into walls). So: anything floor-like faces up, one triangle at a time; walls are turned
            // the way their own Havok body's floors say (flags bits 8-31 are the body).
            var bodyUp = new Dictionary<uint, float>();
            var normals = new Vector3[count];
            for (int i = 0; i < count; i++)
            {
                float* t = (float*)(tris + i * Proto.ColTriBytes);
                uint body = *(uint*)(tris + i * Proto.ColTriBytes + 36) >> 8;
                for (int k = 0; k < 3; k++)
                {
                    vertices[i * 3 + k] = new Vector3(t[k * 3], t[k * 3 + 1], t[k * 3 + 2]);
                }
                Vector3 n = Vector3.Cross(vertices[i * 3 + 1] - vertices[i * 3], vertices[i * 3 + 2] - vertices[i * 3]);
                normals[i] = n;
                float area = n.magnitude;
                if (area > 1e-6f && Mathf.Abs(n.y / area) > 0.5f)
                {
                    bodyUp.TryGetValue(body, out float u);
                    bodyUp[body] = u + n.y;
                }
            }
            bool flip = false;
            for (int i = 0; i < count; i++)
            {
                uint body = *(uint*)(tris + i * Proto.ColTriBytes + 36) >> 8;
                Vector3 n = normals[i];
                float area = n.magnitude;
                bool f;
                if (area > 1e-6f && Mathf.Abs(n.y / area) > 0.3f)
                {
                    f = n.y < 0f;  // floors and slopes face up
                }
                else
                {
                    f = bodyUp.TryGetValue(body, out float u) && u < 0f;
                }
                flip |= f;
                indices[i * 3] = i * 3;
                indices[i * 3 + 1] = f ? i * 3 + 2 : i * 3 + 1;
                indices[i * 3 + 2] = f ? i * 3 + 1 : i * 3 + 2;
            }
            if (flip)
            {
                FlippedBodies++;
            }
            var mesh = new Mesh { name = $"er-body-{id:X}" };
            mesh.indexFormat = vertices.Length > 65000 ? UnityEngine.Rendering.IndexFormat.UInt32 : UnityEngine.Rendering.IndexFormat.UInt16;
            mesh.vertices = vertices;
            mesh.triangles = indices;
            mesh.RecalculateBounds();
            var go = new GameObject($"er-body-{id:X}") { layer = EnvironmentLayer };
            go.transform.SetParent(Root.transform, false);
            var col = go.AddComponent<MeshCollider>();
            col.sharedMesh = mesh;
            if (Plugin.ShowLandsBetween.Value || ShowDebug)
            {
                mesh.RecalculateNormals();
                go.AddComponent<MeshFilter>().sharedMesh = mesh;
                go.AddComponent<MeshRenderer>().sharedMaterial = DebugMaterial;
            }
            return go;
        }

        // Whether Elden Ring's floor is under p (so V1 can be let go there).
        public static bool FloorUnder(Vector3 p, float depth = 60f)
        {
            return Physics.Raycast(p + Vector3.up * 2f, Vector3.down, depth, 1 << EnvironmentLayer, QueryTriggerInteraction.Ignore);
        }

        // Switches off the arena's own level: its renderers (so only V1's things reach the overlay)
        // and its collision (so V1 only touches Elden Ring). Managers, the player and our objects stay.
        public static void StripLevel(Scene scene, Transform player)
        {
            int renderers = 0, colliders = 0;
            foreach (var go in scene.GetRootGameObjects())
            {
                if (player != null && player.root == go.transform)
                {
                    continue;
                }
                foreach (var r in go.GetComponentsInChildren<Renderer>(true))
                {
                    if (player != null && r.transform.IsChildOf(player))
                    {
                        continue;
                    }
                    if (r.enabled)
                    {
                        r.enabled = false;
                        renderers++;
                    }
                }
                foreach (var c in go.GetComponentsInChildren<Collider>(true))
                {
                    if (player != null && c.transform.IsChildOf(player))
                    {
                        continue;
                    }
                    // triggers too: the arena's out-of-bounds and kill zones sit in the same space as
                    // Elden Ring's world, and walking into one launched or teleported V1 (and its
                    // restart threw Elden Ring's collision away)
                    if (c.enabled)
                    {
                        c.enabled = false;
                        colliders++;
                    }
                }
                foreach (var t in go.GetComponentsInChildren<Terrain>(true))
                {
                    t.enabled = false;
                }
                // only V1's sounds (movement, guns, hits) reach Elden Ring: the arena's ambience off
                foreach (var a in go.GetComponentsInChildren<AudioSource>(true))
                {
                    if (player == null || !a.transform.IsChildOf(player))
                    {
                        a.Stop();
                        a.enabled = false;
                    }
                }
            }
            RenderSettings.skybox = null;
            RenderSettings.fog = false;
            Plugin.Log.LogInfo($"arena {scene.name}: switched off {renderers} renderers and {colliders} colliders of its own level");
        }
    }
}
