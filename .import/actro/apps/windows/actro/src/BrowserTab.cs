using System.Drawing;
using Microsoft.Web.WebView2.WinForms;

namespace Actro.Windows;

internal sealed class BrowserTab : IDisposable
{
    private Image? favicon;
    private Image? snapshot;

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
    /// Set immediately before NavigateToString delivers an app-generated
    /// document (internal pages, error pages). NavigationStarting consumes
    /// the flag so that exactly one generated-document navigation may pass
    /// the address allow-list; see NavigationPolicy.IsGeneratedDocumentUri.
    /// </summary>
    public bool HasPendingGeneratedDocument { get; set; }

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

    /// <summary>
    /// Last-seen page snapshot painted by the tab overview, captured on tab
    /// deactivation and refreshed for the active tab at overview open; null
    /// falls back to the themed placeholder card. Ownership matches
    /// <see cref="Favicon"/>: replacing disposes the previous bitmap and
    /// Dispose releases the last one with the tab.
    /// </summary>
    public Image? Snapshot
    {
        get => snapshot;
        set
        {
            if (ReferenceEquals(snapshot, value))
            {
                return;
            }

            snapshot?.Dispose();
            snapshot = value;
        }
    }

    public string DisplayTitle => string.IsNullOrWhiteSpace(Title) ? "New Tab" : Title;

    public void Dispose()
    {
        WebView.Dispose();
        Favicon = null;
        Snapshot = null;
    }
}
