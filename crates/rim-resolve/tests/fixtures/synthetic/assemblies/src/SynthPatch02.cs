// Runtime-patch-shaped assembly: a class-level
// [HarmonyPatch(typeof(TargetAlpha), "DoAlpha")] naming the
// target, and a method-level [HarmonyTranspiler] naming the patch kind -- both
// attribute types resolved from the locally-declared MiniPatchLib.dll
// (../README.md), never the real HarmonyLib. TargetAlpha is a
// separate, deliberately trivial sibling class standing in for the
// "target" a real patch would touch -- only its bare type name is read.
//
// csc /target:library /reference:MiniPatchLib.dll /out:SynthPatch02.dll SynthPatch02.cs

using HarmonyLib;

namespace Example.Patches
{
    public static class TargetAlpha
    {
        public static void DoAlpha()
        {
        }
    }

    [HarmonyPatch(typeof(TargetAlpha), "DoAlpha")]
    public class Patch02
    {
        [HarmonyTranspiler]
        public static void Transpiler()
        {
        }
    }
}
