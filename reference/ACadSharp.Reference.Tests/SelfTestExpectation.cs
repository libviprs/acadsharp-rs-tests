using ACadSharp.Reference.Canonical;

namespace ACadSharp.Reference.Tests;

/// <summary>
/// Rewrites the self-test's committed expectation, and only when told to.
/// </summary>
/// <remarks>
/// <para>
/// The same shape as <c>tools/regenerate_reference.py --accept</c>, for the same
/// reason: the expectation a test compares against must never be updated by the
/// act of running the test, or an ACadSharp change would rewrite the thing it
/// was supposed to be caught by and the suite would go green.
/// </para>
/// <para>
/// So it does nothing at all unless <c>ACADSHARP_REFERENCE_ACCEPT_SELFTEST=1</c>
/// is in the environment, a Rust test asserts no workflow sets that, and it
/// writes into the source tree rather than into the build output so the result
/// is a diff somebody has to look at.
/// </para>
/// <para>
/// To update after a deliberate change:
/// <c>ACADSHARP_REFERENCE_ACCEPT_SELFTEST=1 dotnet test reference/ACadSharp.Reference.sln</c>,
/// then read <c>git diff</c> before committing it.
/// </para>
/// </remarks>
public sealed class SelfTestExpectation
{
    /// <summary>The environment variable that has to be set for anything to be written.</summary>
    public const string AcceptVariable = "ACADSHARP_REFERENCE_ACCEPT_SELFTEST";

    [Fact]
    public void RewritesTheExpectationOnlyWhenExplicitlyAccepted()
    {
        string directory = Path.Combine(
            RepoLayout.Root, "reference", "ACadSharp.Reference.Tests", "Expected");
        string jsonl = Path.Combine(directory, "selftest.reference.jsonl");
        string svg = Path.Combine(directory, "selftest.reference.svg");

        if (Environment.GetEnvironmentVariable(AcceptVariable) != "1")
        {
            // The default path, and the one CI takes. Assert the files exist so
            // this is not a test that does nothing: a missing expectation should
            // fail here rather than three tests later with a file-not-found.
            Assert.True(File.Exists(jsonl), $"{jsonl} is missing");
            Assert.True(File.Exists(svg), $"{svg} is missing");
            return;
        }

        Directory.CreateDirectory(directory);
        var records = SelfTestTests.RoundTrip(SelfTestTests.ArcRadius).Records;
        File.WriteAllBytes(jsonl, CanonicalWriter.Encode(records));
        File.WriteAllBytes(svg, Reference.Svg.SvgWriter.Encode(records));
    }
}
