using System.Net;

namespace Actro.Windows;

internal static class BrowserHomeDocumentPolicy
{
    public const string Title = "Actro";

    public static string BuildDocument(ActroCoreStatus coreStatus)
    {
        var productName = WebUtility.HtmlEncode(coreStatus.ProductName);
        var coreSummary = coreStatus.IsAvailable
            ? $"Rust core ready: C API v{coreStatus.CApiVersion}, contract {coreStatus.SupportedContractName}@v{coreStatus.SupportedContractVersion}, schemas core/runtime {coreStatus.CoreSchemaVersion}/{coreStatus.RuntimeSchemaVersion}."
            : "Rust core DLL is not loaded yet; the Windows shell is running with local fallback metadata.";
        var coreDetail = coreStatus.IsAvailable
            ? "Shared native boundary is available to Windows."
            : coreStatus.UnavailableReason ?? "Native library was not found.";

        return $$"""
        <!doctype html>
        <html>
          <head>
            <meta charset="utf-8">
            <title>{{productName}}</title>
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
              h1 {
                font-size: 28px;
                font-weight: 600;
                margin: 0 0 8px;
              }
              p {
                color: var(--muted-foreground);
                font-size: 15px;
                margin: 0 0 10px;
              }
              code {
                color: var(--muted-foreground);
                font: 13px var(--mono-font);
              }
            </style>
            <script>
              window.addEventListener("DOMContentLoaded", async () => {
                const detail = document.querySelector("[data-core-status]");
                if (!detail) {
                  return;
                }

                if (!window.actroHost || !window.actroHost.getCoreStatus) {
                  detail.textContent = `${detail.textContent}\nHost bridge is not available.`;
                  return;
                }

                try {
                  const status = await window.actroHost.getCoreStatus();
                  const coreState = status.isAvailable ? "ready" : "fallback";
                  detail.textContent = `Host bridge ${coreState}: ${status.productName}, contract ${status.supportedContractName}@v${status.supportedContractVersion}, schemas ${status.coreSchemaVersion}/${status.runtimeSchemaVersion}.`;
                } catch (error) {
                  detail.textContent = `Host bridge error: ${error?.message ?? "unknown error"}`;
                }
              });
            </script>
          </head>
          <body>
            <main>
              <section>
                <h1>{{productName}}</h1>
                <p>{{WebUtility.HtmlEncode(coreSummary)}}</p>
                <code data-core-status>{{WebUtility.HtmlEncode(coreDetail)}}</code>
              </section>
            </main>
          </body>
        </html>
        """;
    }
}
