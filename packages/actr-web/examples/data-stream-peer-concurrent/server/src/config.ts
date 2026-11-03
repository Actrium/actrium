import type { ActorConfig, SwRuntimeConfig } from '@actrium/actr-web';

const loranHttpUrl = (
    import.meta.env.VITE_LORAN_HTTP_URL || 'http://localhost:8081'
).replace(/\/+$/, '');
const iceServers = import.meta.env.VITE_LORAN_STUN_URL
    ? [{ urls: import.meta.env.VITE_LORAN_STUN_URL }]
    : [];

export const runtimeConfig: SwRuntimeConfig = {
    issuer_endpoint: `${loranHttpUrl}/issuer`,
    signaling_url:
        import.meta.env.VITE_LORAN_SIGNALING_URL ||
        `${loranHttpUrl.replace(/^http/, 'ws')}/signaling/ws`,
    realm: 'data-stream@example.com',
    client_actr_type: 'acme/DataStreamPeerConcurrentServer@0.1.0',
    target_actr_type: 'acme/DataStreamPeerConcurrentClient@0.1.0',
    service_fingerprint: '',
    acl_allow_types: ['acme/DataStreamPeerConcurrentClient@0.1.0'],
};

export const actrConfig: ActorConfig = {
    signalingUrl: runtimeConfig.signaling_url,
    realm: runtimeConfig.realm,
    serviceWorkerPath: '/actor.sw.js',
    runtimeConfig,
    iceServers,
    debug: true,
};
