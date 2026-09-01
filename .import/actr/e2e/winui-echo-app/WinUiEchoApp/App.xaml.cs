using Microsoft.UI.Xaml;

namespace WinUiEchoApp;

public partial class App : Application
{
    private MainWindow? _window;

    public App()
    {
        RuntimeLog.Info("App constructor entered");
        InitializeComponent();
        RuntimeLog.Info("App XAML initialized");
    }

    protected override void OnLaunched(LaunchActivatedEventArgs args)
    {
        RuntimeLog.Info("App.OnLaunched entered");
        try
        {
            _window = new MainWindow();
            RuntimeLog.Info("MainWindow constructed");
            _window.Activate();
            RuntimeLog.Info("MainWindow activated");
        }
        catch (Exception ex)
        {
            RuntimeLog.Error("App.OnLaunched failed", ex);
            throw;
        }
    }
}
