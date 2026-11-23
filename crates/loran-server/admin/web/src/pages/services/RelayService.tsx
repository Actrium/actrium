import { ServiceAccessCard } from "./ServiceAccessCard";
import { useEffect, useState } from "react";
import { api, type ServiceDetail } from "../../lib/api";
import { ServicePageLayout, StatusSection } from "../../components/layout/ServicePageLayout";
import { ServiceMetrics } from "./shared";
import { HowItWorks } from "../../components/ui/HowItWorks";

export function RelayService() {
  const [data, setData] = useState<ServiceDetail | null>(null);
  const [error, setError] = useState("");
  useEffect(() => {
    api
      .getServiceDetail("relay")
      .then(setData)
      .catch((e) => setError(e.message));
  }, []);
  const config = data?.config;
  const limits = config?.limits as
    | {
        max_connections?: number;
        max_frame_bytes?: number;
        max_channels_per_endpoint?: number;
        authentication_timeout_secs?: number;
      }
    | undefined;
  return (
    <ServicePageLayout
      title="WSS Relay"
      description="Forwards application data between endpoints over WebSocket channels."
    >
      {error && (
        <p role="alert" className="text-red-700">
          {error}
        </p>
      )}
      {data ? (
        <>
          <StatusSection
            enabled={data.enabled}
            healthy={data.status?.is_healthy}
            disabledHint="The local relay is disabled on this node."
          />
          <ServiceAccessCard service="relay" data={data} />
          <section className="space-y-3 rounded-xl border border-gray-200 bg-white p-5 text-sm">
            <h2 className="font-semibold">Connection limits</h2>
            <dl className="grid grid-cols-[auto_1fr] gap-x-6 gap-y-2">
              <dt>Connection limit</dt>
              <dd>{limits?.max_connections ?? "Unavailable"}</dd>
              <dt>Channels per endpoint</dt>
              <dd>{limits?.max_channels_per_endpoint ?? "Unavailable"}</dd>
              <dt>Authentication timeout</dt>
              <dd>
                {limits?.authentication_timeout_secs == null
                  ? "Unavailable"
                  : limits.authentication_timeout_secs + " s"}
              </dd>
              <dt>Maximum frame size</dt>
              <dd>{limits?.max_frame_bytes ?? "Unavailable"} bytes</dd>
            </dl>
          </section>
          {data.enabled && <ServiceMetrics status={data.status} storageKey="relay" />}
          <HowItWorks storageKey="relay">
            <ol className="list-decimal space-y-2 pl-4 text-xs text-gray-500">
              <li>
                <strong className="text-gray-600">Find the relay.</strong> The SDK reads <code>/relay/config</code>
                {" "}on the Signaling origin and uses the advertised URL (<code>relay.public_url</code>) if set.
                Otherwise it uses <code>/relay/ws</code> on that origin when the local relay is enabled.
              </li>
              <li>
                <strong className="text-gray-600">Authenticate the WebSocket.</strong> In Actor identity mode,
                the opening <code>Hello</code> uses a relay-specific identity from Issuer. Standalone deployments
                can use configured endpoint tokens in static mode.
              </li>
              <li>
                <strong className="text-gray-600">Open a channel.</strong> One endpoint requests a connection and
                the other accepts. The relay forwards frames within the same namespace. Actor identity mode
                also checks App access policy and active endpoint identities.
              </li>
            </ol>
            <p className="mt-4 border-t border-gray-100 pt-3 text-xs text-gray-500">
              The Actr SDK encrypts application payloads end to end before the relay forwards them.
              WSS Relay carries application data. Signaling negotiates WebRTC connections, and TURN carries
              WebRTC traffic when ICE selects a relayed path.
            </p>
          </HowItWorks>
        </>
      ) : (
        !error && <p>Loading relay…</p>
      )}
    </ServicePageLayout>
  );
}
