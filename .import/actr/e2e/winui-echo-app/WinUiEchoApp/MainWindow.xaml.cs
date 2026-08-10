using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Media;

namespace WinUiEchoApp;

public sealed partial class MainWindow : Window
{
    private readonly ActrService _service = new();
    private bool _sending;

    public MainWindow()
    {
        InitializeComponent();
        InputBox.Text = Environment.GetEnvironmentVariable("ACTR_ECHOAPP_TEST_INPUT") ?? "hello";
        Activated += OnActivated;
    }

    private async void OnActivated(object sender, WindowActivatedEventArgs e)
    {
        Activated -= OnActivated;
        var configPath = Environment.GetEnvironmentVariable("ACTR_ECHOAPP_ACTR_CONFIG") ?? "actr.toml";
        try
        {
            await _service.StartIfNeededAsync(configPath);
            StatusText.Text = _service.Status;
            StatusText.Foreground = new SolidColorBrush(Microsoft.UI.Colors.Green);
            SendButton.IsEnabled = true;
            if (_service.ShouldAutoSend() && !string.IsNullOrEmpty(InputBox.Text))
            {
                await SendAsync();
            }
        }
        catch (Exception ex)
        {
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
        try
        {
            var reply = await _service.SendEchoWhenReadyAsync(message ?? InputBox.Text);
            ReplyText.Text = reply;
            ActrService.EmitResult(reply);
        }
        catch (Exception ex)
        {
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
