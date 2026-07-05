using System.Drawing;
using Microsoft.Web.WebView2.WinForms;

namespace Actro.Windows;

internal sealed class BrowserTab : IDisposable
{
    private Image? favicon;

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

    /// <summary>
    /// Cached favicon painted by the sidebar; null falls back to the
    /// letter avatar. The tab owns the image: replacing it disposes the
    /// previous one, and Dispose releases the last one with the tab.
    /// </summary>
    public Image? Favicon
    {
        get => favicon;
        set
        {
            if (ReferenceEquals(favicon, value))
            {
                return;
            }

            favicon?.Dispose();
            favicon = value;
        }
    }

    public string DisplayTitle => string.IsNullOrWhiteSpace(Title) ? "New Tab" : Title;

    public void Dispose()
    {
        WebView.Dispose();
        Favicon = null;
    }
}
