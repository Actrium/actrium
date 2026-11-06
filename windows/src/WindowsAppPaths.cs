namespace Actrium.Windows;

internal static class WindowsAppPaths
{
    // Environment override for the durable state root. When set, it replaces
    // the whole per-user root so two concurrently running instances can keep
    // fully independent state trees (WebView2 profiles, sync stores, SQLite),
    // which is otherwise impossible because every store routes through this
    // single choke point. Program.Main mirrors the --user-data-root argument
    // into this variable so both entry points share one source of truth.
    public const string UserDataRootEnvironmentVariable = "ACTRIUM_USER_DATA_ROOT";

    public static string GetLocalAppDataRoot()
    {
        var overrideRoot = Environment.GetEnvironmentVariable(UserDataRootEnvironmentVariable);
        if (!string.IsNullOrWhiteSpace(overrideRoot))
        {
            var trimmed = overrideRoot.Trim();
            // A relative override would resolve against the process working
            // directory and silently split state across instances (or worse,
            // land under a transient CWD). Fail loudly instead of guessing,
            // mirroring the local-folder root normalization elsewhere.
            if (!Path.IsPathRooted(trimmed))
            {
                throw new InvalidOperationException(
                    $"{UserDataRootEnvironmentVariable} must be an absolute path but was '{overrideRoot}'.");
            }

            // Returned as the full root: callers that need isolation (for
            // example BrowserProfilePolicy) Path.Combine their own subfolder
            // on top, so the standard Actrium/Actrium/Windows suffix is not
            // appended here on purpose.
            return trimmed;
        }

        return GetDefaultLocalAppDataRoot(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData));
    }

    internal static string GetDefaultLocalAppDataRoot(string localApplicationData)
    {
        return Path.Combine(
            localApplicationData,
            "Actrium",
            "Actrium",
            "Windows");
    }
}
