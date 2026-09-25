import { useState } from 'react';
import { ShieldAlert } from 'lucide-react';
import { identityApi, type IdentityKind, type IdentityKeys, type IdentityView } from '../../lib/api';
import { ConfirmModal } from '../ui/ConfirmModal';

const LABELS: Record<IdentityKind, { title: string; placeholder: string }> = {
  'realm-domains': { title: 'Realm domain identity', placeholder: 'acme.com' },
  publishers: { title: 'Publisher identity', placeholder: 'apps.example.com' },
};

function KeyList({ title, keys }: { title: string; keys: IdentityKeys }) {
  return (
    <div className="flex-1 min-w-0">
      <div className="text-xs font-semibold text-gray-600 uppercase mb-1">
        {title} <span className="font-normal normal-case text-gray-400">(sequence {keys.sequence})</span>
      </div>
      <table className="w-full text-xs">
        <tbody className="divide-y divide-gray-100">
          {keys.keys.map(key => (
            <tr key={`${key.id}-${key.fingerprint}`}>
              <td className="py-1 pr-2 font-mono text-gray-800">{key.id}</td>
              <td className="py-1 pr-2 text-gray-500">{key.status}</td>
              <td className="py-1 font-mono text-[11px] text-gray-600 break-all">{key.fingerprint}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

/**
 * Compare the pinned signing keys of a Realm domain or publisher with the
 * ones it serves now, and accept the served ones after an identity change.
 * Fingerprints are SHA-256 of the public keys; compare them with the domain
 * owner out of band before accepting.
 */
export function IdentityReview({ kind, disabled = false }: { kind: IdentityKind; disabled?: boolean }) {
  const [name, setName] = useState('');
  const [view, setView] = useState<IdentityView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [confirming, setConfirming] = useState(false);

  const check = async () => {
    setLoading(true);
    setError(null);
    setNotice(null);
    setView(null);
    try { setView(await identityApi.get(kind, name.trim())); }
    catch (e) { setError(e instanceof Error ? e.message : String(e)); }
    finally { setLoading(false); }
  };

  const accept = async () => {
    if (!view) return;
    setLoading(true);
    try {
      await identityApi.confirm(kind, name.trim(), view.fetched.digest);
      setNotice(`The served keys of ${name.trim()} are now pinned.`);
      setView(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
      setConfirming(false);
    }
  };

  return (
    <div className="bg-white rounded-xl border border-gray-200 p-4 space-y-3">
      <div className="flex items-center gap-2 font-semibold text-gray-800">
        <ShieldAlert size={16} /> {LABELS[kind].title}
      </div>
      <p className="text-xs text-gray-500">
        Signing keys that do not chain to the pinned keys are refused. Compare the fingerprints
        with the domain owner before accepting the served keys; this is recorded in the audit log.
      </p>
      <div className="flex gap-2">
        <input
          value={name}
          onChange={e => { setName(e.target.value); setView(null); }}
          placeholder={LABELS[kind].placeholder}
          className="flex-1 rounded-lg border border-gray-300 px-3 py-1.5 text-sm font-mono"
        />
        <button
          onClick={() => void check()}
          disabled={loading || name.trim() === ''}
          className="px-3 py-1.5 text-sm bg-blue-600 text-white rounded-lg hover:bg-blue-700 disabled:opacity-50"
        >Check</button>
      </div>
      {error && <div className="text-xs text-red-600">{error}</div>}
      {notice && <div className="text-xs text-green-700">{notice}</div>}
      {view && (
        <div className="space-y-3">
          <div className="text-sm">
            {view.outcome === 'accepted' && <span className="text-green-700">The served keys are accepted under the pin; nothing to confirm.</span>}
            {view.outcome === 'identity_change' && <span className="text-red-700">Identity change: the served keys are not signed by a pinned key.</span>}
            {view.outcome === 'rejected' && <span className="text-red-700">Refused under the pin: {view.reason}</span>}
          </div>
          <div className="flex gap-6">
            <KeyList title="Pinned" keys={view.pinned} />
            <KeyList title="Served" keys={view.fetched} />
          </div>
          {view.outcome !== 'accepted' && (
            <button
              onClick={() => setConfirming(true)}
              disabled={disabled || loading}
              title={disabled ? 'Managed by superv' : undefined}
              className="px-3 py-1.5 text-sm bg-red-600 text-white rounded-lg hover:bg-red-700 disabled:opacity-50"
            >Accept new identity</button>
          )}
        </div>
      )}
      <ConfirmModal
        isOpen={confirming}
        title="Accept new identity"
        message={
          <span>
            Replace the pinned keys of <strong>{name.trim()}</strong> with the served ones?
            <br />
            <br />
            <span className="text-xs text-gray-500 block">
              Only do this after the domain owner confirmed the served fingerprints. Documents signed
              by the old keys are no longer accepted.
            </span>
          </span>
        }
        confirmLabel="Accept"
        isDestructive
        loading={loading}
        onConfirm={() => void accept()}
        onCancel={() => setConfirming(false)}
      />
    </div>
  );
}
