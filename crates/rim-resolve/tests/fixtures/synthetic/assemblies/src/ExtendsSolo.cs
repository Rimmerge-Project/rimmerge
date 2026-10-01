// "extends T from A", single-owner variant (see ExportsSolo.cs): three
// dependents (019-021) each ship a copy, all extending
// `Example.ExportsSolo.SoloWidget` -- a Hard, unambiguous `AssemblyRef`
// to "exportssolo" from each, giving `example.framework03` exactly 3
// real Hard dependents.
//
// csc /target:library /reference:ExportsSolo.dll /out:ExtendsSolo.dll ExtendsSolo.cs

using Example.ExportsSolo;

namespace Example.ExtendsSolo
{
    public class DerivedSoloWidget : SoloWidget
    {
        public override void Configure()
        {
            Value = 2;
        }
    }
}
