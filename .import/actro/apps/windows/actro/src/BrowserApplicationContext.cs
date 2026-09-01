using System.Windows.Forms;

namespace Actro.Windows;

internal sealed class BrowserApplicationContext : ApplicationContext
{
    private readonly Dictionary<MainForm, string> privateUserDataFolders = [];
    private int openWindowCount;

    public BrowserApplicationContext(WindowsStartupOptions startupOptions)
    {
        var isPrivateStartup = startupOptions.IsPrivate;
        OpenWindow(
            startupOptions,
            isPrivateStartup
                ? BrowserProfilePolicy.GetPrivateUserDataFolder(Guid.NewGuid())
                : BrowserProfilePolicy.GetDefaultUserDataFolder(),
            restoreSession: !isPrivateStartup,
            persistSession: !isPrivateStartup,
            restoreWindowState: !isPrivateStartup,
            persistWindowState: !isPrivateStartup,
            isPrivateWindow: isPrivateStartup);
    }

    private void OpenWindow(
        WindowsStartupOptions startupOptions,
        string userDataFolder,
        bool restoreSession,
        bool persistSession,
        bool restoreWindowState,
        bool persistWindowState,
        bool isPrivateWindow)
    {
        var window = new MainForm(
            startupOptions,
            BrowserSessionStore.CreateDefault(),
            BrowserWindowStateStore.CreateDefault(),
            userDataFolder,
            restoreSession,
            persistSession,
            restoreWindowState,
            persistWindowState,
            isPrivateWindow);
        window.NewWindowRequested += OnNewWindowRequested;
        window.NewPrivateWindowRequested += OnNewPrivateWindowRequested;
        window.FormClosed += OnWindowClosed;
        if (isPrivateWindow)
        {
            privateUserDataFolders[window] = userDataFolder;
        }

        openWindowCount++;
        window.Show();
    }

    private void OnNewWindowRequested(object? sender, BrowserNewWindowRequestedEventArgs args)
    {
        OpenWindow(
            new WindowsStartupOptions(args.InitialUrl),
            BrowserProfilePolicy.GetDefaultUserDataFolder(),
            restoreSession: false,
            persistSession: false,
            restoreWindowState: false,
            persistWindowState: false,
            isPrivateWindow: false);
    }

    private void OnNewPrivateWindowRequested(object? sender, BrowserNewWindowRequestedEventArgs args)
    {
        OpenWindow(
            new WindowsStartupOptions(args.InitialUrl),
            BrowserProfilePolicy.GetPrivateUserDataFolder(Guid.NewGuid()),
            restoreSession: false,
            persistSession: false,
            restoreWindowState: false,
            persistWindowState: false,
            isPrivateWindow: true);
    }

    private void OnWindowClosed(object? sender, FormClosedEventArgs args)
    {
        if (sender is MainForm window)
        {
            window.NewWindowRequested -= OnNewWindowRequested;
            window.NewPrivateWindowRequested -= OnNewPrivateWindowRequested;
            window.FormClosed -= OnWindowClosed;
            if (privateUserDataFolders.Remove(window, out var privateUserDataFolder))
            {
                BrowserProfilePolicy.TryDeleteUserDataFolder(privateUserDataFolder);
            }
        }

        openWindowCount--;
        if (openWindowCount == 0)
        {
            ExitThread();
        }
    }
}
