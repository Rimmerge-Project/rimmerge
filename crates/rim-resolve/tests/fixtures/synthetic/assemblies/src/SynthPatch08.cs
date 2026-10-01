// Runtime-patch-shaped assembly: a class-level
// [HarmonyPatch(typeof(TargetDelta), "DoDelta")] naming the
// target, and a method-level [HarmonyPostfix] naming the patch kind -- both
// attribute types resolved from the locally-declared MiniPatchLib.dll
// (../README.md), never the real HarmonyLib. TargetDelta is a
// separate, deliberately trivial sibling class standing in for the
// "target" a real patch would touch -- only its bare type name is read.
//
// csc /target:library /reference:MiniPatchLib.dll /out:SynthPatch08.dll SynthPatch08.cs

using HarmonyLib;

namespace Example.Patches
{
    public static class TargetDelta
    {
        public static void DoDelta()
        {
        }
    }

    [HarmonyPatch(typeof(TargetDelta), "DoDelta")]
    public class Patch08
    {
        [HarmonyPostfix]
        public static void Postfix()
        {
        }
    }
}
