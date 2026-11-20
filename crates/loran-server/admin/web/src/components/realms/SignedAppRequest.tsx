import { useId, useState } from "react";
import { api } from "../../lib/api";
import { ManagementDisclosure, managementButton, managementCard } from "../ui/ManagementControls";

export function SignedAppRequest({
  appId,
  disabled,
  onAccepted,
}: {
  appId?: string;
  disabled: boolean;
  onAccepted: () => void;
}) {
  const [id, setId] = useState(appId || "");
  const [document, setDocument] = useState("");
  const [result, setResult] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [expanded, setExpanded] = useState(false);
  const panelId = useId();
  return (
    <div className={managementCard + " p-5"}>
      <ManagementDisclosure expanded={expanded} controls={panelId} onClick={() => setExpanded(value => !value)} className="w-full">
        External domain · signed requests
      </ManagementDisclosure>
      <div id={panelId} hidden={!expanded}>
        <p className="my-3 text-sm text-gray-600">
          The domain holder signs requests with actr. Submit the signed create request, save the returned draft, then
          submit the signed profile to activate the app and receive its first Owner invite. The same workflow
          supports profile updates, retirement and Owner recovery.
        </p>
        <form
          className="space-y-3"
          onSubmit={async (e) => {
            e.preventDefault();
            setError("");
            setResult("");
            setBusy(true);
            try {
              const signed = JSON.parse(document);
              if (signed.realm !== id || !signed.signature)
                throw Error("The signed request must name this App ID and include its domain signature.");
              const response = await api.signedAppRequest(id, document);
              setResult(JSON.stringify(response, null, 2));
              setDocument("");
              onAccepted();
            } catch (err) {
              setError(err instanceof Error ? err.message : "The signed request failed.");
            } finally {
              setBusy(false);
            }
          }}
        >
          <label className="grid gap-1 text-sm">
            External App ID
            <input
              className="rounded border p-2"
              required
              readOnly={Boolean(appId)}
              value={id}
              onChange={(e) => {
                setId(e.target.value.trim());
                setResult("");
              }}
            />
          </label>
          <label className="grid gap-1 text-sm">
            Signed request JSON
            <textarea
              aria-label="Signed request JSON"
              required
              rows={5}
              className="rounded border p-2 font-mono text-xs"
              value={document}
              onChange={(e) => setDocument(e.target.value)}
            />
          </label>
          <button disabled={disabled || busy} className={managementButton.secondary}>
            Submit signed request
          </button>
        </form>
        {error && (
          <p role="alert" className="mt-2 text-sm text-red-700">
            {error}
          </p>
        )}
        {result && (
          <div className="mt-3 space-y-2">
            <label className="grid gap-1 text-sm">
              Domain response · save any Owner invite now
              <textarea className="rounded border p-2 font-mono text-xs" rows={6} readOnly value={result} />
            </label>
            <button className="text-sm text-blue-700" onClick={() => setResult("")}>
              Clear response
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
