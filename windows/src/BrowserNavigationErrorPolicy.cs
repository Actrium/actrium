using System.Net;

namespace Actrium.Windows;

internal static class BrowserNavigationErrorPolicy
{
    public const string DefaultTitle = "Navigation failed";
    public const string DefaultMessage = "Actrium could not open this page.";
    public const string UnknownAddress = "Unknown address";
    public const string UnknownStatus = "Unknown error";

    public static string FormatStatus(string? errorStatus)
    {
        var status = NormalizeStatus(errorStatus);
        return string.IsNullOrWhiteSpace(status)
            ? DefaultTitle
            : $"{DefaultTitle}: {status}";
    }

    public static string BuildErrorDocument(string? address, string? errorStatus)
    {
        var safeAddress = WebUtility.HtmlEncode(NormalizeAddress(address));
        var safeStatus = WebUtility.HtmlEncode(NormalizeStatus(errorStatus, UnknownStatus));

        return $$"""
        <!doctype html>
        <html>
          <head>
            <meta charset="utf-8">
            <title>{{WebUtility.HtmlEncode(DefaultTitle)}}</title>
            <style>
              {{BrowserDocumentStylePolicy.RootCss}}
              html, body {
                height: 100%;
                margin: 0;
                font-family: var(--ui-font);
                background: var(--page-background);
                color: var(--page-foreground);
              }
              main {
                box-sizing: border-box;
                display: grid;
                min-height: 100%;
                place-items: center;
                padding: 48px;
              }
              section {
                max-width: 680px;
                width: 100%;
              }
              h1 {
                font-size: 28px;
                font-weight: 600;
                margin: 0 0 8px;
              }
              p {
                color: var(--muted-foreground);
                font-size: 15px;
                margin: 0 0 24px;
              }
              dl {
                display: grid;
                gap: 8px;
                margin: 0;
              }
              dt {
                color: var(--muted-foreground);
                font-size: 12px;
                font-weight: 600;
                text-transform: uppercase;
              }
              dd {
                margin: 0 0 14px;
                overflow-wrap: anywhere;
              }
              code {
                color: var(--muted-foreground);
                font: 13px var(--mono-font);
              }
            </style>
          </head>
          <body>
            <main>
              <section>
                <h1>{{WebUtility.HtmlEncode(DefaultTitle)}}</h1>
                <p>{{WebUtility.HtmlEncode(DefaultMessage)}}</p>
                <dl>
                  <dt>Address</dt>
                  <dd><code>{{safeAddress}}</code></dd>
                  <dt>Error</dt>
                  <dd><code>{{safeStatus}}</code></dd>
                </dl>
              </section>
            </main>
          </body>
        </html>
        """;
    }

    /// <summary>
    /// ConnectionAborted and OperationCanceled mark navigations that were
    /// superseded or intentionally stopped (a newer navigation started,
    /// the user pressed Stop, or the allow-list blocked it after posting
    /// its own status). Chrome shows no error page for those - the tab
    /// keeps its current document - and replacing the pending newer
    /// navigation with an error document would clobber it.
    /// </summary>
    public static bool ShouldShowErrorDocument(string? errorStatus)
    {
        var status = NormalizeStatus(errorStatus);
        return !string.Equals(status, "ConnectionAborted", StringComparison.OrdinalIgnoreCase)
            && !string.Equals(status, "OperationCanceled", StringComparison.OrdinalIgnoreCase);
    }

    private static string NormalizeAddress(string? address)
    {
        return string.IsNullOrWhiteSpace(address) ? UnknownAddress : address.Trim();
    }

    private static string NormalizeStatus(string? errorStatus, string fallback = "")
    {
        return string.IsNullOrWhiteSpace(errorStatus) ? fallback : errorStatus.Trim();
    }
}
