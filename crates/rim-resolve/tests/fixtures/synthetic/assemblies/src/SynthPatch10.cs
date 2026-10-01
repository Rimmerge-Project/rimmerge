// Runtime-patch-shaped assembly: a class-level
// [HarmonyPatch(typeof(TargetEpsilon), "DoEpsilon")] naming the
// target, and a method-level [HarmonyPostfix] naming the patch kind -- both
// attribute types resolved from the locally-declared MiniPatchLib.dll
// (../README.md), never the real HarmonyLib. TargetEpsilon is a
// separate, deliberately trivial sibling class standing in for the
// "target" a real patch would touch -- only its bare type name is read.
//
// csc /target:library /reference:MiniPatchLib.dll /out:SynthPatch10.dll SynthPatch10.cs

using HarmonyLib;

namespace Example.Patches
{
    public static class TargetEpsilon
    {
        public static void DoEpsilon()
        {
        }
    }

    [HarmonyPatch(typeof(TargetEpsilon), "DoEpsilon")]
    public class Patch10
    {
        [HarmonyPostfix]
        public static void Postfix()
        {
        }
    }
}
