namespace Actro.Windows;

internal static class BrowserProfilePolicy
{
    public const string UserDataFolderName = "WebView2";
    public const string PrivateUserDataParentFolderName = "PrivateWebView2";

    public static string GetDefaultUserDataFolder()
    {
        return Path.Combine(WindowsAppPaths.GetLocalAppDataRoot(), UserDataFolderName);
    }

    public static string GetPrivateUserDataFolder(Guid windowId)
    {
        return Path.Combine(
            WindowsAppPaths.GetLocalAppDataRoot(),
            PrivateUserDataParentFolderName,
            windowId.ToString("N"));
    }

    public static string EnsureUserDataFolder(string userDataFolder)
    {
        if (string.IsNullOrWhiteSpace(userDataFolder))
        {
            throw new ArgumentException("A WebView2 user data folder is required.", nameof(userDataFolder));
        }

        Directory.CreateDirectory(userDataFolder);
        return userDataFolder;
    }

    public static bool TryDeleteUserDataFolder(string userDataFolder)
    {
        if (string.IsNullOrWhiteSpace(userDataFolder) || !Directory.Exists(userDataFolder))
        {
            return false;
        }

        try
        {
            Directory.Delete(userDataFolder, recursive: true);
            return true;
        }
        catch (IOException)
        {
            return false;
        }
        catch (UnauthorizedAccessException)
        {
            return false;
        }
    }
}
