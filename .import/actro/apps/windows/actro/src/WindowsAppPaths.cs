namespace Actro.Windows;

internal static class WindowsAppPaths
{
    public static string GetLocalAppDataRoot()
    {
        return Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "Actrium",
            "Actro",
            "Windows");
    }
}
