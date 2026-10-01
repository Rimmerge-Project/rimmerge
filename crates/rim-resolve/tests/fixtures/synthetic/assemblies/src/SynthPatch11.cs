// Runtime-patch-shaped assembly: a class-level
// [HarmonyPatch(typeof(TargetZeta), "DoZeta")] naming the
// target, and a method-level [HarmonyPrefix] naming the patch kind -- both
// attribute types resolved from the locally-declared MiniPatchLib.dll
// (../README.md), never the real HarmonyLib. TargetZeta is a
// separate, deliberately trivial sibling class standing in for the
// "target" a real patch would touch -- only its bare type name is read.
//
// csc /target:library /reference:MiniPatchLib.dll /out:SynthPatch11.dll SynthPatch11.cs

using HarmonyLib;

namespace Example.Patches
{
    public static class TargetZeta
    {
        public static void DoZeta()
        {
        }
    }

    [HarmonyPatch(typeof(TargetZeta), "DoZeta")]
    public class Patch11
    {
        [HarmonyPrefix]
        public static void Prefix()
        {
        }
    }
}
