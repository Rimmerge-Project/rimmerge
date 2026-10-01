// The higher-version half of the `AssemblyVersionPrecedence` pair:
// identical shape to `Exports.cs`,
// compiled to the *same* output file name (`Exports.dll`, so the
// `Assembly` table name is still "exports" -- the duplicate-assembly and
// any-of constructs this synthetic install already plants need the name
// unchanged) but into `../bin/versioned/`, and stamped 1.1.0.0 instead of
// 1.0.0.0. Shipped by `example.framework02` only; `example.framework01`
// keeps the plain `../bin/Exports.dll` (1.0.0.0).
//
// csc /target:library /out:versioned\Exports.dll ExportsVersioned.cs

using System.Reflection;

[assembly: AssemblyVersion("1.1.0.0")]

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
