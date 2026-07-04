using Microsoft.Web.WebView2.WinForms;

namespace Actro.Windows;

internal sealed class BrowserTab : IDisposable
{
    public BrowserTab(Guid? id = null)
    {
        Id = id ?? Guid.NewGuid();
        WebView = new WebView2
        {
            Dock = DockStyle.Fill,
        };
    }

    public Guid Id { get; }

    public WebView2 WebView { get; }

    public HostMessageBridge? HostMessageBridge { get; set; }

    public string Title { get; set; } = "New Tab";

    public string Address { get; set; } = NavigationPolicy.HomeAddress;

    public bool IsLoading { get; set; }

    public bool IsShowingErrorPage { get; set; }

    public string DisplayTitle => string.IsNullOrWhiteSpace(Title) ? "New Tab" : Title;

    public void Dispose()
    {
        WebView.Dispose();
    }
}
