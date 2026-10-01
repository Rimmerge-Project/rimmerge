// "Exports type T": a base type another synthetic assembly can extend
// (`ExtendsHard.cs`, producing a `TypeDef.Extends` -> `AssemblyRef` load-time
// reference) or merely reference inside a method body (`CallsSoft.cs`,
// producing a lazy reference). See ../README.md.
//
// An explicit `AssemblyVersion` (not the CLR's own `0.0.0.0` default for
// an attribute-less build) so `ExportsVersioned.cs`'s own 1.1.0.0 copy has
// a real, non-zero version to be strictly *higher than* --
// `assembly_version_precedence_edges` drops any `0.0.0.0` owner from
// consideration entirely (its own doc comment: not a real, strictly-lower
// version, just an unversioned build), so two owners both need an
// explicit version for this edge to ever fire.
//
// csc /target:library /out:Exports.dll Exports.cs

using System.Reflection;

[assembly: AssemblyVersion("1.0.0.0")]

namespace Example.Exports
{
    public class BaseWidget
    {
        public int Value;

        public virtual void Configure()
        {
            Value = 1;
        }
    }
}
