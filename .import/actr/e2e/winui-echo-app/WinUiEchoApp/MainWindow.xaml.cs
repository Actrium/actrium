using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Media;

namespace WinUiEchoApp;

public sealed partial class MainWindow : Window
{
    private readonly ActrService _service = new();
    private bool _sending;

    public MainWindow()
    {
        RuntimeLog.Info("MainWindow constructor entered");
        InitializeComponent();
        RuntimeLog.Info("MainWindow XAML initialized");
        InputBox.Text = Environment.GetEnvironmentVariable("ACTR_ECHOAPP_TEST_INPUT") ?? "hello";
        Activated += OnActivated;
        RuntimeLog.Info("MainWindow activation handler attached");
    }

    private async void OnActivated(object sender, WindowActivatedEventArgs e)
    {
        Activated -= OnActivated;
        var configPath = Environment.GetEnvironmentVariable("ACTR_ECHOAPP_ACTR_CONFIG") ?? "actr.toml";
        RuntimeLog.Info($"MainWindow activated; config={configPath}");
        try
        {
            await _service.StartIfNeededAsync(configPath);
            RuntimeLog.Info("ACTR node started");
            StatusText.Text = _service.Status;
            StatusText.Foreground = new SolidColorBrush(Microsoft.UI.Colors.Green);
            SendButton.IsEnabled = true;
            if (_service.ShouldAutoSend() && !string.IsNullOrEmpty(InputBox.Text))
            {
                RuntimeLog.Info("Sending automatic echo request");
                await SendAsync();
            }
        }
        catch (Exception ex)
        {
            RuntimeLog.Error("MainWindow activation failed", ex);
            StatusText.Text = _service.Status;
            StatusText.Foreground = new SolidColorBrush(Microsoft.UI.Colors.Red);
            ReplyText.Text = $"Echo failed: {ex.Message}";
            ActrService.EmitResult($"Echo failed: {ex.Message}");
        }
    }

    private void SendButton_Click(object sender, RoutedEventArgs e) => _ = SendAsync();

    private async Task SendAsync(string? message = null)
    {
        if (_sending) return;
        _sending = true;
        SendButton.IsEnabled = false;
        ReplyText.Text = string.Empty;
        RuntimeLog.Info($"Sending echo request; length={(message ?? InputBox.Text).Length}");
        try
        {
            var reply = await _service.SendEchoWhenReadyAsync(message ?? InputBox.Text);
            RuntimeLog.Info($"Echo reply received; length={reply.Length}");
            ReplyText.Text = reply;
            ActrService.EmitResult(reply);
        }
        catch (Exception ex)
        {
            RuntimeLog.Error("Echo request failed", ex);
            ReplyText.Text = $"Echo failed: {ex.Message}";
            ActrService.EmitResult($"Echo failed: {ex.Message}");
        }
        finally
        {
            _sending = false;
            SendButton.IsEnabled = _service.IsReady;
        }
    }
}
