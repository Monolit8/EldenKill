using System;
using BepInEx;
using BepInEx.Configuration;
using BepInEx.Logging;
using HarmonyLib;
using UnityEngine;
using UnityEngine.SceneManagement;

namespace EldenKill
{
    // ULTRAKILL's half of EldenKill: V1 is simulated here, in a stripped-down arena whose level
    // geometry is replaced by Elden Ring's collision, and Elden Ring shows V1's view on top of its
    // own world. Elden Ring's DLL (host-eldenring) starts this process hidden and owns the link.
    [BepInPlugin("dev.eldenkill.guest", "EldenKill", "0.1.0")]
    public sealed class Plugin : BaseUnityPlugin
    {
        internal static ManualLogSource Log;
        internal static ConfigEntry<string> ArenaScene;
        internal static ConfigEntry<bool> AutoLoadArena;
        internal static ConfigEntry<float> OverlayScale;
        internal static ConfigEntry<bool> ChromaKey;
        internal static ConfigEntry<float> DamageToEldenRing;
        internal static ConfigEntry<bool> ShowLandsBetween;
        internal static ConfigEntry<bool> UnlockWeapons;
        internal static ConfigEntry<int> Fps;
        internal static ConfigEntry<float> SoundVolume;
        internal static ConfigEntry<bool> Diagnostics;

        // Started by Elden Ring with this argument: no window of its own.
        internal static readonly bool LaunchedHidden = Array.Exists(Environment.GetCommandLineArgs(), a => a == "-eldenkill-hidden");

        // Started by practice-ultrakill.bat with this argument: plain ULTRAKILL straight into the
        // Sandbox with every weapon, never linked to Elden Ring (even while EldenKill runs).
        internal static readonly bool Practice = Array.Exists(Environment.GetCommandLineArgs(), a => a == "-ultrakill-practice");

        private void Awake()
        {
            Log = Logger;
            ArenaScene = Config.Bind("Guest", "ArenaScene", "uk_construct",
                "The ULTRAKILL scene V1 lives in while linked (its level geometry is switched off). uk_construct is the Sandbox.");
            AutoLoadArena = Config.Bind("Guest", "AutoLoadArena", true,
                "Load ArenaScene by itself once Elden Ring is linked (from the main menu).");
            OverlayScale = Config.Bind("Overlay", "Scale", 1.0f,
                "ULTRAKILL renders V1's view at Elden Ring's resolution times this (0.5 halves the copying cost).");
            ChromaKey = Config.Bind("Overlay", "ChromaKey", true,
                "Clear ULTRAKILL's background to pure magenta (FF00FF) and let Elden Ring key it out. " +
                "ULTRAKILL's post-processing doesn't keep alpha, so this is the default.");
            DamageToEldenRing = Config.Bind("Combat", "DamageToEldenRing", 1.0f,
                "Multiplier on V1's damage before Elden Ring converts it to HP (the host has its own scale too).");
            Fps = Config.Bind("Guest", "Fps", 80,
                "ULTRAKILL's frame rate while started by Elden Ring (keep it at Elden Ring's: fps in eldenkill.ini).");
            SoundVolume = Config.Bind("Guest", "SoundVolume", -1.0f,
                "ULTRAKILL's sound effects volume while linked (0-1); below 0 (the default): ULTRAKILL's own Audio options decide.");
            UnlockWeapons = Config.Bind("Combat", "AllWeapons", true,
                "While linked to Elden Ring, V1 has every weapon whether or not your save has unlocked it. Your save file isn't changed.");
            ShowLandsBetween = Config.Bind("Debug", "ShowLandsBetween", false,
                "Draw Elden Ring's collision in ULTRAKILL (grey wireframe-ish meshes) instead of only colliding with it.");
            Diagnostics = Config.Bind("Debug", "Diagnostics", false, "Extra logging.");

            Application.runInBackground = true;
            new Harmony("dev.eldenkill.guest").PatchAll(typeof(Plugin).Assembly);

            EnsureGuest();
            // ULTRAKILL's scene loader disables scripts outside DontDestroyOnLoad (Killcraft found
            // this): re-check after every load.
            SceneManager.sceneLoaded += (_, __) => EnsureGuest();
            Log.LogInfo($"EldenKill guest loaded{(LaunchedHidden ? " (hidden, started by Elden Ring)" : Practice ? " (practice: Sandbox only, no Elden Ring)" : "")}");
        }

        private static Guest guest;

        private static void EnsureGuest()
        {
            if (guest == null)
            {
                var go = new GameObject("EldenKill");
                go.hideFlags = HideFlags.HideInHierarchy;
                guest = go.AddComponent<Guest>();
            }
            if (guest.gameObject.scene.name != "DontDestroyOnLoad")
            {
                DontDestroyOnLoad(guest.gameObject);
            }
            if (!guest.enabled)
            {
                guest.enabled = true;
            }
        }
    }
}
