namespace WinUiEchoApp;

internal static class RuntimeLog
{
    private const string LogPathVariable = "ACTR_ECHOAPP_LOG_PATH";
    private static readonly object Gate = new();

    internal static void Info(string message) => Write("INFO", message, null);

    internal static void Error(string message, Exception exception) =>
        Write("ERROR", message, exception);

    private static void Write(string level, string message, Exception? exception)
    {
        try
        {
            var path = Environment.GetEnvironmentVariable(LogPathVariable);
            if (string.IsNullOrWhiteSpace(path))
            {
                path = Path.Combine(AppContext.BaseDirectory, "winui-runtime.log");
            }

            var directory = Path.GetDirectoryName(path);
            if (!string.IsNullOrEmpty(directory))
            {
                Directory.CreateDirectory(directory);
            }

            var detail = exception is null ? message : $"{message}: {exception}";
            var line = $"{DateTimeOffset.UtcNow:O} [{level}] pid={Environment.ProcessId} {detail}{Environment.NewLine}";
            lock (Gate)
            {
                File.AppendAllText(path, line);
            }
        }
        catch
        {
            // Startup logging must never prevent the WinUI app from launching.
        }
    }
}
