using System.Collections;
using Unity.Collections;
using Unity.Collections.LowLevel.Unsafe;
using UnityEngine;
using UnityEngine.Rendering;

namespace EldenKill
{
    // V1's view (arms, guns, projectiles, explosions, ULTRAKILL's HUD) for Elden Ring to draw on
    // top of its world: the finished frame is grabbed at the end of every frame, read back without
    // stalling (AsyncGPUReadback) and copied into the link's overlay triple buffer.
    // The arena's own level is switched off and the background cleared to the key colour, so the
    // host can cut V1's things out (SkyCraft's overlay does the same with Minecraft's hand and HUD).
    internal sealed unsafe class OverlayOut : MonoBehaviour
    {
        public static readonly Color KeyColor = new Color(1f, 0f, 1f, 0f);
        /// ULTRAKILL's own menu is open: the whole frame goes over Elden Ring, on a dark background
        /// (its see-through panels blended with the key colour into purple boxes)
        public static bool Opaque;

        public bool Active;
        // a ring of capture textures, so up to three readbacks are in flight (one at a time halved
        // the overlay's frame rate: a readback takes a frame or two to come back)
        private readonly RenderTexture[] grabs = new RenderTexture[3];
        private int next;
        private int pending;
        private ulong frameId;
        private readonly WaitForEndOfFrame endOfFrame = new WaitForEndOfFrame();

        public static ulong FramesSent { get; private set; }

        private void OnEnable()
        {
            StartCoroutine(Loop());
        }

        private IEnumerator Loop()
        {
            while (true)
            {
                yield return endOfFrame;
                if (!Active || !Link.Ready || pending >= grabs.Length)
                {
                    continue;
                }
                int w = Screen.width, h = Screen.height;
                if (w <= 0 || h <= 0 || w > Proto.OverlayMaxW || h > Proto.OverlayMaxH)
                {
                    continue;
                }
                ref RenderTexture grab = ref grabs[next];
                next = (next + 1) % grabs.Length;
                if (grab == null || grab.width != w || grab.height != h)
                {
                    if (grab != null)
                    {
                        grab.Release();
                    }
                    grab = new RenderTexture(w, h, 0, RenderTextureFormat.ARGB32);
                }
                ScreenCapture.CaptureScreenshotIntoRenderTexture(grab);
                pending++;
                AsyncGPUReadback.Request(grab, 0, TextureFormat.RGBA32, OnReadback);
            }
        }

        private void OnReadback(AsyncGPUReadbackRequest req)
        {
            pending--;
            if (req.hasError || !Link.Ready)
            {
                return;
            }
            NativeArray<byte> data = req.GetData<byte>();
            int w = req.width, h = req.height;
            if (data.Length < w * h * 4)
            {
                return;
            }
            UnsafeUtility.MemCpy(Link.OverlayBackPixels, NativeArrayUnsafeUtility.GetUnsafeReadOnlyPtr(data), (long)w * h * 4);
            // Read back from the captured texture, rows come out bottom-up except where UVs start at
            // the top (D3D), where the capture is itself upside down: checked with the fake host's dump
            uint flags = (SystemInfo.graphicsUVStartsAtTop ? 0 : Proto.OvBottomUp) | (Plugin.ChromaKey.Value && !Opaque ? Proto.OvChromaKey : 0);
            Link.PublishOverlay(w, h, flags, ++frameId);
            FramesSent++;
        }

        private void OnDestroy()
        {
            foreach (var grab in grabs)
            {
                if (grab != null)
                {
                    grab.Release();
                }
            }
        }
    }
}
