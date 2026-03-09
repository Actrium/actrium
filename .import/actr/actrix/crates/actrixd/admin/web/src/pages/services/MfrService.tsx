import { useState, useEffect, useCallback } from 'react';
import { Building2, CheckCircle, Clock, XCircle, AlertTriangle, Package, Key, Copy } from 'lucide-react';
import { mfrApi, type Manufacturer, type ActrPackage, type MfrKeychain } from '../../lib/api';

// Status badge component
function StatusBadge({ status }: { status: Manufacturer['status'] }) {
  const config = {
    active: { color: 'bg-green-100 text-green-800', icon: CheckCircle, label: 'Active' },
    pending: { color: 'bg-yellow-100 text-yellow-800', icon: Clock, label: 'Pending' },
    suspended: { color: 'bg-orange-100 text-orange-800', icon: AlertTriangle, label: 'Suspended' },
    revoked: { color: 'bg-red-100 text-red-800', icon: XCircle, label: 'Revoked' },
  }[status];
  const Icon = config.icon;
  return (
    <span className={`inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-medium ${config.color}`}>
      <Icon size={12} />
      {config.label}
    </span>
  );
}

// Keychain modal - shows private key once after approval
function KeychainModal({ keychain, onClose }: { keychain: MfrKeychain; onClose: () => void }) {
  const [copied, setCopied] = useState(false);
  const copy = async () => {
    await navigator.clipboard.writeText(JSON.stringify(keychain, null, 2));
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };
  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
      <div className="bg-white rounded-xl shadow-xl p-6 max-w-2xl w-full mx-4">
        <div className="flex items-center gap-2 mb-4">
          <Key className="text-amber-500" size={20} />
          <h2 className="text-lg font-semibold">MFR Keychain Issued</h2>
        </div>
        <div className="bg-amber-50 border border-amber-200 rounded-lg p-3 mb-4 text-sm text-amber-800">
          ⚠️ Save this private key immediately. It will never be shown again.
        </div>
        <pre className="bg-gray-900 text-green-400 rounded-lg p-4 text-xs overflow-auto max-h-64 font-mono">
          {JSON.stringify(keychain, null, 2)}
        </pre>
        <div className="flex gap-2 mt-4">
          <button
            onClick={copy}
            className="flex items-center gap-2 px-4 py-2 bg-gray-800 text-white rounded-lg text-sm hover:bg-gray-700"
          >
            <Copy size={14} />
            {copied ? 'Copied!' : 'Copy JSON'}
          </button>
          <button
            onClick={onClose}
            className="ml-auto px-4 py-2 border border-gray-300 rounded-lg text-sm hover:bg-gray-50"
          >
            Close
          </button>
        </div>
      </div>
    </div>
  );
}

export function MfrService() {
  const [manufacturers, setManufacturers] = useState<Manufacturer[]>([]);
  const [packages, setPackages] = useState<ActrPackage[]>([]);
  const [selectedMfr, setSelectedMfr] = useState<Manufacturer | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [keychain, setKeychain] = useState<MfrKeychain | null>(null);
  const [actionLoading, setActionLoading] = useState<number | null>(null);

  const loadData = useCallback(async () => {
    try {
      const [mfrs, pkgs] = await Promise.all([
        mfrApi.list(),
        mfrApi.listPackages(),
      ]);
      setManufacturers(mfrs);
      setPackages(pkgs);
      setError(null);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { void loadData(); }, [loadData]);

  const handleApprove = async (mfr: Manufacturer) => {
    setActionLoading(mfr.id);
    try {
      const kc = await mfrApi.approve(mfr.id);
      setKeychain(kc);
      await loadData();
    } catch (e) { setError(String(e)); }
    finally { setActionLoading(null); }
  };

  const handleSuspend = async (mfr: Manufacturer) => {
    if (!confirm(`Suspend "${mfr.name}"?`)) return;
    setActionLoading(mfr.id);
    try { await mfrApi.suspend(mfr.id); await loadData(); }
    catch (e) { setError(String(e)); }
    finally { setActionLoading(null); }
  };

  const handleReinstate = async (mfr: Manufacturer) => {
    setActionLoading(mfr.id);
    try { await mfrApi.reinstate(mfr.id); await loadData(); }
    catch (e) { setError(String(e)); }
    finally { setActionLoading(null); }
  };

  const handleDelete = async (mfr: Manufacturer) => {
    if (!confirm(`Delete "${mfr.name}" and all its packages? This cannot be undone.`)) return;
    setActionLoading(mfr.id);
    try { await mfrApi.delete(mfr.id); await loadData(); }
    catch (e) { setError(String(e)); }
    finally { setActionLoading(null); }
  };

  const handleRevokePackage = async (pkg: ActrPackage) => {
    if (!confirm(`Revoke package "${pkg.type_str}"?`)) return;
    try { await mfrApi.revokePackage(pkg.id); await loadData(); }
    catch (e) { setError(String(e)); }
  };

  const stats = {
    total: manufacturers.length,
    active: manufacturers.filter(m => m.status === 'active').length,
    pending: manufacturers.filter(m => m.status === 'pending').length,
    suspended: manufacturers.filter(m => m.status === 'suspended').length,
  };

  const filteredPackages = selectedMfr
    ? packages.filter(p => p.mfr_id === selectedMfr.id)
    : packages;

  const ts = (t: number) => new Date(t * 1000).toLocaleDateString();

  if (loading) return <div className="p-8 text-gray-500">Loading...</div>;

  return (
    <div className="p-6 space-y-6">
      {keychain && <KeychainModal keychain={keychain} onClose={() => setKeychain(null)} />}

      <div>
        <h1 className="text-2xl font-bold text-gray-900 flex items-center gap-2">
          <Building2 size={24} /> Manufacturer Registry
        </h1>
        <p className="text-gray-500 text-sm mt-1">Manage registered actor manufacturers and published packages.</p>
      </div>

      {error && (
        <div className="bg-red-50 border border-red-200 rounded-lg p-3 text-sm text-red-700">{error}</div>
      )}

      {/* Stats */}
      <div className="grid grid-cols-4 gap-4">
        {[
          { label: 'Total', value: stats.total, color: 'text-gray-700' },
          { label: 'Active', value: stats.active, color: 'text-green-700' },
          { label: 'Pending', value: stats.pending, color: 'text-yellow-700' },
          { label: 'Suspended', value: stats.suspended, color: 'text-orange-700' },
        ].map(s => (
          <div key={s.label} className="bg-white rounded-xl border border-gray-200 p-4">
            <div className={`text-2xl font-bold ${s.color}`}>{s.value}</div>
            <div className="text-gray-500 text-sm">{s.label}</div>
          </div>
        ))}
      </div>

      {/* MFR Table */}
      <div className="bg-white rounded-xl border border-gray-200 overflow-hidden">
        <div className="px-4 py-3 border-b border-gray-100 flex items-center justify-between">
          <h2 className="font-semibold text-gray-800">Manufacturers</h2>
          {selectedMfr && (
            <button onClick={() => setSelectedMfr(null)} className="text-xs text-gray-500 hover:text-gray-800">
              Clear filter
            </button>
          )}
        </div>
        <table className="w-full text-sm">
          <thead className="bg-gray-50 text-gray-500 text-xs uppercase">
            <tr>
              {['Name', 'Domain', 'Status', 'Verified', 'Packages', 'Actions'].map(h => (
                <th key={h} className="px-4 py-2 text-left font-medium">{h}</th>
              ))}
            </tr>
          </thead>
          <tbody className="divide-y divide-gray-100">
            {manufacturers.length === 0 && (
              <tr><td colSpan={6} className="px-4 py-8 text-center text-gray-400">No manufacturers registered</td></tr>
            )}
            {manufacturers.map(mfr => {
              const pkgCount = packages.filter(p => p.mfr_id === mfr.id).length;
              const isSelected = selectedMfr?.id === mfr.id;
              return (
                <tr
                  key={mfr.id}
                  className={`hover:bg-gray-50 cursor-pointer ${isSelected ? 'bg-blue-50' : ''}`}
                  onClick={() => setSelectedMfr(isSelected ? null : mfr)}
                >
                  <td className="px-4 py-3 font-mono font-medium text-gray-900">{mfr.name}</td>
                  <td className="px-4 py-3 text-gray-600">{mfr.domain}</td>
                  <td className="px-4 py-3"><StatusBadge status={mfr.status} /></td>
                  <td className="px-4 py-3 text-gray-500">{mfr.verified_at ? ts(mfr.verified_at) : '—'}</td>
                  <td className="px-4 py-3">
                    <span className="inline-flex items-center gap-1 text-gray-600">
                      <Package size={12} /> {pkgCount}
                    </span>
                  </td>
                  <td className="px-4 py-3" onClick={e => e.stopPropagation()}>
                    <div className="flex gap-1">
                      {mfr.status === 'pending' && (
                        <button
                          onClick={() => void handleApprove(mfr)}
                          disabled={actionLoading === mfr.id}
                          className="px-2 py-1 text-xs bg-green-600 text-white rounded hover:bg-green-700 disabled:opacity-50"
                        >Approve</button>
                      )}
                      {mfr.status === 'active' && (
                        <button
                          onClick={() => void handleSuspend(mfr)}
                          disabled={actionLoading === mfr.id}
                          className="px-2 py-1 text-xs bg-orange-500 text-white rounded hover:bg-orange-600 disabled:opacity-50"
                        >Suspend</button>
                      )}
                      {mfr.status === 'suspended' && (
                        <button
                          onClick={() => void handleReinstate(mfr)}
                          disabled={actionLoading === mfr.id}
                          className="px-2 py-1 text-xs bg-blue-600 text-white rounded hover:bg-blue-700 disabled:opacity-50"
                        >Reinstate</button>
                      )}
                      <button
                        onClick={() => void handleDelete(mfr)}
                        disabled={actionLoading === mfr.id}
                        className="px-2 py-1 text-xs bg-red-500 text-white rounded hover:bg-red-600 disabled:opacity-50"
                      >Delete</button>
                    </div>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>

      {/* Package Table */}
      <div className="bg-white rounded-xl border border-gray-200 overflow-hidden">
        <div className="px-4 py-3 border-b border-gray-100">
          <h2 className="font-semibold text-gray-800">
            {selectedMfr ? `Packages — ${selectedMfr.name}` : 'All Packages'}
          </h2>
        </div>
        <table className="w-full text-sm">
          <thead className="bg-gray-50 text-gray-500 text-xs uppercase">
            <tr>
              {['Type', 'Manufacturer', 'Status', 'Published', 'Actions'].map(h => (
                <th key={h} className="px-4 py-2 text-left font-medium">{h}</th>
              ))}
            </tr>
          </thead>
          <tbody className="divide-y divide-gray-100">
            {filteredPackages.length === 0 && (
              <tr><td colSpan={5} className="px-4 py-8 text-center text-gray-400">No packages</td></tr>
            )}
            {filteredPackages.map(pkg => (
              <tr key={pkg.id} className="hover:bg-gray-50">
                <td className="px-4 py-3 font-mono text-gray-900">{pkg.type_str}</td>
                <td className="px-4 py-3 text-gray-600">{pkg.manufacturer}</td>
                <td className="px-4 py-3">
                  <span className={`inline-flex items-center px-2 py-0.5 rounded-full text-xs font-medium ${
                    pkg.status === 'active' ? 'bg-green-100 text-green-800' : 'bg-red-100 text-red-800'
                  }`}>{pkg.status}</span>
                </td>
                <td className="px-4 py-3 text-gray-500">{ts(pkg.published_at)}</td>
                <td className="px-4 py-3">
                  {pkg.status === 'active' && (
                    <button
                      onClick={() => void handleRevokePackage(pkg)}
                      className="px-2 py-1 text-xs bg-red-500 text-white rounded hover:bg-red-600"
                    >Revoke</button>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
