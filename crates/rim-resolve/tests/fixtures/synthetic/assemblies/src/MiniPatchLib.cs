// A minimal, locally-declared stand-in for HarmonyLib: just enough of
// the real library's own public shape (`HarmonyPatch`'s `(Type, string)`
// constructor, and the three bare kind-marker attributes) for
// `rim_analyzer::extract::pe_metadata`'s attribute reader to recognize, without
// needing the real `0Harmony.dll` (which this workspace has no license
// to redistribute). See `../README.md`.
//
// csc /target:library /out:MiniPatchLib.dll MiniPatchLib.cs

using System;

namespace HarmonyLib
{
    public class HarmonyPatch : Attribute
    {
        public HarmonyPatch(Type targetType, string methodName)
        {
        }
    }

    public class HarmonyPrefix : Attribute
    {
    }

    public class HarmonyPostfix : Attribute
    {
    }

    public class HarmonyTranspiler : Attribute
    {
    }
}
