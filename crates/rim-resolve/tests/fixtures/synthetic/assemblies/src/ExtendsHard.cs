// "extends T from A": `DerivedWidget` extends `Example.Exports.BaseWidget`,
// so `TypeDef.Extends` names a `TypeRef` resolving to `Exports`'s own
// `AssemblyRef` — a load-time (Hard) reference. See ../README.md.
//
// csc /target:library /reference:Exports.dll /out:ExtendsHard.dll ExtendsHard.cs

using Example.Exports;

namespace Example.Extends
{
    public class DerivedWidget : BaseWidget
    {
        public override void Configure()
        {
            Value = 2;
        }
    }
}
