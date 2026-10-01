// "calls T from A only in a method body": `Caller.Do` references
// `Example.Exports.BaseWidget` only inside a method body — never in
// `TypeDef.Extends`/`InterfaceImpl`, so the resulting `AssemblyRef` to
// `Exports` resolves lazily (Soft). See ../README.md.
//
// csc /target:library /reference:Exports.dll /out:CallsSoft.dll CallsSoft.cs

using Example.Exports;

namespace Example.Calls
{
    public class Caller
    {
        public int Do()
        {
            var widget = new BaseWidget();
            widget.Configure();
            return widget.Value;
        }
    }
}
