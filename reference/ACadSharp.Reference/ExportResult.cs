namespace ACadSharp.Reference;

/// <summary>
/// The process exit codes, one per failure the brief names in section 3.
/// </summary>
/// <remarks>
/// Distinct codes rather than a blanket 1, because the regeneration tool and CI
/// both act on them: a reader failure on one fixture is a finding about
/// ACadSharp, and an invalid output path is a finding about the person who ran
/// the command.
/// </remarks>
public enum ExitCode
{
    /// <summary>Everything asked for was produced.</summary>
    Ok = 0,

    /// <summary>The command line did not parse, or asked for something impossible.</summary>
    Usage = 2,

    /// <summary>The DWG could not be read.</summary>
    ReaderFailure = 3,

    /// <summary>The file is a version this ACadSharp will not read.</summary>
    UnsupportedVersion = 4,

    /// <summary>An output path does not exist, is not writable, or is not a file.</summary>
    InvalidOutputPath = 5,

    /// <summary>A record could not be encoded, for example because it carried a NaN.</summary>
    CanonicalSerializationFailure = 6,

    /// <summary>The SVG could not be generated.</summary>
    SvgGenerationFailure = 7,

    /// <summary>ACadSharp threw something the tool does not classify.</summary>
    UnhandledAcadSharpError = 8,
}

/// <summary>Why an operation did not produce a value.</summary>
/// <param name="Code">The exit code the CLI should return.</param>
/// <param name="Message">
/// A message that names the fixture or the path, because it is the only thing
/// the person reading a red CI job gets.
/// </param>
public sealed record ExportError(ExitCode Code, string Message);

/// <summary>
/// Either a value or an <see cref="ExportError"/>.
/// </summary>
/// <typeparam name="T">The value type.</typeparam>
/// <remarks>
/// Expected failure is a return value here, not an exception. A DWG that will
/// not read and an entity that cannot be represented are both ordinary
/// outcomes of running this tool over a corpus, and the CLI's whole job is to
/// turn one into an exit code and a line of text. Exceptions stay for
/// programmer error.
/// </remarks>
public readonly record struct Result<T>
{
    private readonly T? _value;

    private Result(T? value, ExportError? error)
    {
        _value = value;
        Error = error;
    }

    /// <summary>The failure, or <see langword="null"/> when there was none.</summary>
    public ExportError? Error { get; }

    /// <summary>Whether there is a value.</summary>
    public bool IsOk => Error is null;

    /// <summary>The value. Only valid when <see cref="IsOk"/>.</summary>
    /// <exception cref="InvalidOperationException">The result is a failure.</exception>
    public T Value => IsOk
        ? _value!
        : throw new InvalidOperationException(
            $"read the value of a failed Result: {Error!.Code} {Error.Message}");

    internal static Result<T> Create(T? value, ExportError? error) => new(value, error);
}

/// <summary>
/// Builds <see cref="Result{T}"/> values.
/// </summary>
/// <remarks>
/// Separate from the generic type so the factories are not static members on a
/// generic, which is awkward to call and which the analyzers rightly flag.
/// </remarks>
public static class Result
{
    /// <summary>A successful result.</summary>
    /// <typeparam name="T">The value type.</typeparam>
    /// <param name="value">The value.</param>
    /// <returns>The result.</returns>
    public static Result<T> Ok<T>(T value) => Result<T>.Create(value, null);

    /// <summary>A failed result.</summary>
    /// <typeparam name="T">The value type that was expected.</typeparam>
    /// <param name="code">The exit code.</param>
    /// <param name="message">What went wrong, naming the input.</param>
    /// <returns>The result.</returns>
    public static Result<T> Fail<T>(ExitCode code, string message) =>
        Result<T>.Create(default, new ExportError(code, message));

    /// <summary>A failed result carrying an existing error.</summary>
    /// <typeparam name="T">The value type that was expected.</typeparam>
    /// <param name="error">The error.</param>
    /// <returns>The result.</returns>
    public static Result<T> Fail<T>(ExportError error) => Result<T>.Create(default, error);
}
