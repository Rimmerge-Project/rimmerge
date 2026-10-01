// "exports type T", single-owner variant: shipped only by
// `example.framework03` -- unlike `Exports.dll`
// (deliberately shipped by two owners, so every reference to it is
// ambiguous and resolves to an `AnyOf` constraint, never a plain edge),
// exactly one mod ships this assembly, so `ExtendsSolo.dll`'s own Hard
// reference to it is unambiguous: a real `AssemblyRef` edge, and enough
// distinct dependents (3) push `example.framework03` past
// `FRAMEWORK_HARD_DEPENDENT_THRESHOLD` (3, `framework_score.rs`) so
// `is_framework_candidate` fires too -- both previously disclosed gaps.
//
// csc /target:library /out:ExportsSolo.dll ExportsSolo.cs

namespace Example.ExportsSolo
{
    public class SoloWidget
    {
        public int Value;

        public virtual void Configure()
        {
            Value = 1;
        }
    }
}
