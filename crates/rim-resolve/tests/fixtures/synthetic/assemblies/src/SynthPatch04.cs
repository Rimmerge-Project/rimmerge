// Runtime-patch-shaped assembly: a class-level
// [HarmonyPatch(typeof(TargetBeta), "DoBeta")] naming the
// target, and a method-level [HarmonyTranspiler] naming the patch kind -- both
// attribute types resolved from the locally-declared MiniPatchLib.dll
// (../README.md), never the real HarmonyLib. TargetBeta is a
// separate, deliberately trivial sibling class standing in for the
// "target" a real patch would touch -- only its bare type name is read.
//
// csc /target:library /reference:MiniPatchLib.dll /out:SynthPatch04.dll SynthPatch04.cs

using HarmonyLib;

namespace Example.Patches
{
    public static class TargetBeta
    {
        public static void DoBeta()
        {
        }
    }

    [HarmonyPatch(typeof(TargetBeta), "DoBeta")]
    public class Patch04
    {
        [HarmonyTranspiler]
        public static void Transpiler()
        {
        }
    }
}
