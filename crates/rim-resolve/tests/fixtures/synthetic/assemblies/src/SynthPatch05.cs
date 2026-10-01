// Runtime-patch-shaped assembly: a class-level
// [HarmonyPatch(typeof(TargetGamma), "DoGamma")] naming the
// target, and a method-level [HarmonyPrefix] naming the patch kind -- both
// attribute types resolved from the locally-declared MiniPatchLib.dll
// (../README.md), never the real HarmonyLib. TargetGamma is a
// separate, deliberately trivial sibling class standing in for the
// "target" a real patch would touch -- only its bare type name is read.
//
// csc /target:library /reference:MiniPatchLib.dll /out:SynthPatch05.dll SynthPatch05.cs

using HarmonyLib;

namespace Example.Patches
{
    public static class TargetGamma
    {
        public static void DoGamma()
        {
        }
    }

    [HarmonyPatch(typeof(TargetGamma), "DoGamma")]
    public class Patch05
    {
        [HarmonyPrefix]
        public static void Prefix()
        {
        }
    }
}
