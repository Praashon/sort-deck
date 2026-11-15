import { useState } from 'react';
import {
  AlertTriangle,
  ArrowDown,
  ArrowUp,
  Layers,
  ListFilter,
  Pencil,
  Plus,
  RotateCcw,
  Trash2,
} from 'lucide-react';
import { EmptyState } from '../components/common/EmptyState';
import { SkeletonBlock } from '../components/common/SkeletonBlock';
import { Switch } from '../components/common/Switch';
import { ConfirmDialog } from '../components/dialogs/ConfirmDialog';
import { RuleEditorModal, type RuleFormValues } from '../components/rules/RuleEditorModal';
import {
  StarterPacksModal,
  type StarterPackDefinition,
} from '../components/rules/StarterPacksModal';
import { useAppData } from '../hooks/useAppData';
import { useToast } from '../hooks/useToast';
import { messages } from '../i18n';
import * as ipc from '../services/ipc';
import type { Rule } from '../types/generated';
import { formatBytes } from '../utils/format';
import { resolveErrorMessage } from '../utils/paths';

export function Rules() {
  const { loading, rulesData, setRulesData, refreshAll } = useAppData();
  const { pushToast } = useToast();

  const [editorOpen, setEditorOpen] = useState(false);
  const [editingRule, setEditingRule] = useState<Rule | null>(null);
  const [savingRule, setSavingRule] = useState(false);
  const [editorError, setEditorError] = useState<string | null>(null);

  const [deleteTarget, setDeleteTarget] = useState<Rule | null>(null);
  const [deleting, setDeleting] = useState(false);

  const [restoreOpen, setRestoreOpen] = useState(false);
  const [restoring, setRestoring] = useState(false);
  const [starterPacksOpen, setStarterPacksOpen] = useState(false);

  const handleOpenCreate = () => {
    setEditingRule(null);
    setEditorError(null);
    setEditorOpen(true);
  };

  const handleOpenEdit = (rule: Rule) => {
    setEditingRule(rule);
    setEditorError(null);
    setEditorOpen(true);
  };

  const handleSaveRule = async (values: RuleFormValues) => {
    setSavingRule(true);
    setEditorError(null);
    try {
      if (editingRule) {
        await ipc.updateRule({
          id: editingRule.id,
          name: values.name,
          destination: values.destination,
          extensions: values.extensions,
          enabled: values.enabled,
          nameContains: values.nameContains,
          minSizeBytes: values.minSizeBytes,
          maxSizeBytes: values.maxSizeBytes,
          minAgeDays: values.minAgeDays,
        });
      } else {
        await ipc.createRule(values);
      }
      const nextList = await ipc.listRules();
      setRulesData(nextList);
      setEditorOpen(false);
      pushToast('success', messages.rules.savedToast);
    } catch (err) {
      setEditorError(resolveErrorMessage(err));
    } finally {
      setSavingRule(false);
    }
  };

  const handleToggleEnabled = async (rule: Rule) => {
    try {
      await ipc.updateRule({
        id: rule.id,
        name: rule.name,
        destination: rule.destination,
        extensions: rule.extensions,
        enabled: !rule.enabled,
        nameContains: rule.nameContains,
        minSizeBytes: rule.minSizeBytes,
        maxSizeBytes: rule.maxSizeBytes,
        minAgeDays: rule.minAgeDays,
      });
      const nextList = await ipc.listRules();
      setRulesData(nextList);
    } catch (err) {
      pushToast('error', resolveErrorMessage(err));
    }
  };

  const handleInstallPack = async (pack: StarterPackDefinition) => {
    try {
      const existingNames = new Set(
        (rulesData?.rules ?? []).map((r) => r.name.trim().toLowerCase()),
      );
      for (const ruleReq of pack.rules) {
        if (!existingNames.has(ruleReq.name.trim().toLowerCase())) {
          await ipc.createRule(ruleReq);
          existingNames.add(ruleReq.name.trim().toLowerCase());
        }
      }
      const nextList = await ipc.listRules();
      setRulesData(nextList);
      setStarterPacksOpen(false);
      pushToast('success', messages.starterPacks.packAddedToast);
    } catch (err) {
      pushToast('error', resolveErrorMessage(err));
    }
  };

  const handleMovePriority = async (index: number, delta: -1 | 1) => {
    const targetIndex = index + delta;
    if (targetIndex < 0 || targetIndex >= rulesData.rules.length) {
      return;
    }
    const ordered = [...rulesData.rules];
    const current = ordered[index];
    const swap = ordered[targetIndex];
    if (!current || !swap) {
      return;
    }
    ordered[index] = swap;
    ordered[targetIndex] = current;

    try {
      const nextList = await ipc.reorderRules({
        orderedRuleIds: ordered.map((r) => r.id),
      });
      setRulesData(nextList);
      pushToast('info', messages.rules.reorderedToast);
    } catch (err) {
      pushToast('error', resolveErrorMessage(err));
    }
  };

  const handleConfirmDelete = async () => {
    if (!deleteTarget) {
      return;
    }
    setDeleting(true);
    try {
      await ipc.deleteRule(deleteTarget.id);
      const nextList = await ipc.listRules();
      setRulesData(nextList);
      setDeleteTarget(null);
      pushToast('info', messages.rules.deletedToast);
    } catch (err) {
      pushToast('error', resolveErrorMessage(err));
    } finally {
      setDeleting(false);
    }
  };

  const handleConfirmRestoreDefaults = async () => {
    setRestoring(true);
    try {
      const nextList = await ipc.restoreDefaultRules();
      setRulesData(nextList);
      await refreshAll();
      setRestoreOpen(false);
      pushToast('info', messages.rules.restoredToast);
    } catch (err) {
      pushToast('error', resolveErrorMessage(err));
    } finally {
      setRestoring(false);
    }
  };

  return (
    <div className="space-y-6">
      <header className="flex flex-wrap items-center justify-between gap-4 border-b border-border-subtle pb-4">
        <div>
          <h1 className="text-lg font-semibold tracking-tight text-ink">{messages.rules.title}</h1>
          <p className="mt-0.5 text-xs text-ink-muted">{messages.rules.subtitle}</p>
        </div>

        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={() => setStarterPacksOpen(true)}
            className="inline-flex items-center gap-1.5 rounded-md border border-border-subtle bg-surface-elevated px-3 py-1.5 text-xs font-medium text-ink transition-colors hover:bg-surface-hover"
          >
            <Layers className="h-3.5 w-3.5" aria-hidden="true" />
            <span>{messages.rules.starterPacksButton}</span>
          </button>

          <button
            type="button"
            onClick={() => setRestoreOpen(true)}
            className="inline-flex items-center gap-1.5 rounded-md border border-border-subtle bg-surface-elevated px-3 py-1.5 text-xs font-medium text-ink transition-colors hover:bg-surface-hover"
          >
            <RotateCcw className="h-3.5 w-3.5" aria-hidden="true" />
            <span>{messages.rules.restoreDefaultsButton}</span>
          </button>

          <button
            type="button"
            onClick={handleOpenCreate}
            className="inline-flex items-center gap-1.5 rounded-md bg-primary px-3.5 py-1.5 text-xs font-medium text-primary-ink transition-colors hover:bg-primary-hover"
          >
            <Plus className="h-3.5 w-3.5" aria-hidden="true" />
            <span>{messages.rules.newRuleButton}</span>
          </button>
        </div>
      </header>

      {rulesData.overlaps.length > 0 && (
        <section
          aria-labelledby="overlap-warnings-heading"
          className="rounded-lg border border-warning-border bg-warning-bg px-4 py-3 text-xs text-warning-ink"
        >
          <div className="flex items-start gap-2.5">
            <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" aria-hidden="true" />
            <div className="space-y-1">
              <h2 id="overlap-warnings-heading" className="font-semibold">
                {messages.rules.overlapTitle}
              </h2>
              <ul className="space-y-1">
                {rulesData.overlaps.map((warn) => (
                  <li key={warn.extension}>
                    <span className="font-mono font-semibold">.{warn.extension}</span>:{' '}
                    {messages.rules.overlapWinnerPrefix} <strong>{warn.winnerRuleName}</strong> (
                    {messages.rules.overlapShadowedPrefix} {warn.shadowedRuleIds.join(', ')})
                  </li>
                ))}
              </ul>
            </div>
          </div>
        </section>
      )}

      {loading ? (
        <SkeletonBlock rows={5} />
      ) : rulesData.rules.length === 0 ? (
        <EmptyState
          icon={ListFilter}
          title={messages.rules.emptyTitle}
          description={messages.rules.emptyDesc}
          action={
            <button
              type="button"
              onClick={() => setRestoreOpen(true)}
              className="inline-flex items-center gap-1.5 rounded-md bg-primary px-3 py-1.5 text-xs font-medium text-primary-ink"
            >
              <RotateCcw className="h-3.5 w-3.5" aria-hidden="true" />
              <span>{messages.rules.restoreDefaultsButton}</span>
            </button>
          }
        />
      ) : (
        <div className="overflow-x-auto rounded-lg border border-border-subtle bg-surface-elevated">
          <table className="w-full border-collapse text-left text-xs">
            <thead>
              <tr className="border-b border-border-subtle bg-surface-inset text-[11px] font-medium text-ink-muted">
                <th scope="col" className="px-4 py-2.5">
                  {messages.rules.colPriority}
                </th>
                <th scope="col" className="px-4 py-2.5">
                  {messages.rules.colName}
                </th>
                <th scope="col" className="px-4 py-2.5">
                  {messages.rules.colExtensions}
                </th>
                <th scope="col" className="px-4 py-2.5">
                  {messages.rules.colDestination}
                </th>
                <th scope="col" className="px-4 py-2.5">
                  {messages.rules.colState}
                </th>
                <th scope="col" className="px-4 py-2.5 text-right">
                  {messages.rules.colActions}
                </th>
              </tr>
            </thead>
            <tbody className="divide-y divide-border-subtle">
              {rulesData.rules.map((rule, index) => (
                <tr key={rule.id} className="hover:bg-surface-hover">
                  <td className="px-4 py-3 whitespace-nowrap">
                    <div className="inline-flex items-center gap-1">
                      <span className="tabular-nums w-6 font-mono font-medium text-ink-secondary">
                        #{rule.priority}
                      </span>
                      <button
                        type="button"
                        disabled={index === 0}
                        onClick={() => void handleMovePriority(index, -1)}
                        aria-label={`${messages.rules.moveUpAria}: ${rule.name}`}
                        className="rounded border border-border-subtle bg-surface p-1 text-ink-muted transition-colors hover:text-ink disabled:opacity-30"
                      >
                        <ArrowUp className="h-3 w-3" aria-hidden="true" />
                      </button>
                      <button
                        type="button"
                        disabled={index === rulesData.rules.length - 1}
                        onClick={() => void handleMovePriority(index, 1)}
                        aria-label={`${messages.rules.moveDownAria}: ${rule.name}`}
                        className="rounded border border-border-subtle bg-surface p-1 text-ink-muted transition-colors hover:text-ink disabled:opacity-30"
                      >
                        <ArrowDown className="h-3 w-3" aria-hidden="true" />
                      </button>
                    </div>
                  </td>

                  <td className="px-4 py-3">
                    <div className="flex items-center gap-2">
                      <span className="font-medium text-ink">{rule.name}</span>
                      <span className="rounded border border-border-subtle bg-surface-inset px-1.5 py-0.5 text-[10px] text-ink-muted">
                        {rule.isDefault ? messages.rules.defaultBadge : messages.rules.customBadge}
                      </span>
                    </div>
                    {(rule.nameContains !== null ||
                      rule.minSizeBytes !== null ||
                      rule.maxSizeBytes !== null ||
                      rule.minAgeDays !== null) && (
                      <div className="mt-1.5 flex flex-wrap gap-1.5">
                        {rule.nameContains !== null && (
                          <span className="rounded border border-border-subtle bg-surface-inset px-1.5 py-0.5 text-[10px] text-ink-muted">
                            {messages.rules.filterBadgeContains}: &quot;{rule.nameContains}&quot;
                          </span>
                        )}
                        {rule.minSizeBytes !== null && (
                          <span className="rounded border border-border-subtle bg-surface-inset px-1.5 py-0.5 text-[10px] text-ink-muted">
                            {messages.rules.filterBadgeMinSize}: {formatBytes(rule.minSizeBytes)}
                          </span>
                        )}
                        {rule.maxSizeBytes !== null && (
                          <span className="rounded border border-border-subtle bg-surface-inset px-1.5 py-0.5 text-[10px] text-ink-muted">
                            {messages.rules.filterBadgeMaxSize}: {formatBytes(rule.maxSizeBytes)}
                          </span>
                        )}
                        {rule.minAgeDays !== null && (
                          <span className="rounded border border-border-subtle bg-surface-inset px-1.5 py-0.5 text-[10px] text-ink-muted">
                            {messages.rules.filterBadgeMinAge} {rule.minAgeDays}{' '}
                            {messages.rules.daysUnit}
                          </span>
                        )}
                      </div>
                    )}
                  </td>

                  <td className="px-4 py-3">
                    <div className="flex max-w-xs flex-wrap gap-1">
                      {rule.extensions.map((ext) => (
                        <span
                          key={ext}
                          className="rounded border border-border-subtle bg-surface-inset px-1.5 py-0.5 font-mono text-[11px] text-ink-secondary"
                        >
                          .{ext}
                        </span>
                      ))}
                    </div>
                  </td>

                  <td className="px-4 py-3 font-mono text-ink-secondary">{rule.destination}</td>

                  <td className="px-4 py-3">
                    <Switch
                      compact
                      checked={rule.enabled}
                      onChange={() => void handleToggleEnabled(rule)}
                      label={`${messages.rules.toggleRuleAria}: ${rule.name}`}
                    />
                  </td>

                  <td className="px-4 py-3 text-right whitespace-nowrap">
                    <div className="inline-flex items-center gap-1.5">
                      <button
                        type="button"
                        onClick={() => handleOpenEdit(rule)}
                        aria-label={`${messages.rules.editRuleAria}: ${rule.name}`}
                        className="rounded border border-border-subtle bg-surface p-1.5 text-ink-muted transition-colors hover:bg-surface-hover hover:text-ink"
                      >
                        <Pencil className="h-3.5 w-3.5" aria-hidden="true" />
                      </button>
                      <button
                        type="button"
                        onClick={() => setDeleteTarget(rule)}
                        aria-label={`${messages.rules.deleteRuleAria}: ${rule.name}`}
                        className="rounded border border-border-subtle bg-surface p-1.5 text-ink-muted transition-colors hover:border-danger-border hover:bg-danger-bg hover:text-danger-ink"
                      >
                        <Trash2 className="h-3.5 w-3.5" aria-hidden="true" />
                      </button>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      <StarterPacksModal
        open={starterPacksOpen}
        existingRules={rulesData?.rules ?? []}
        onInstallPack={handleInstallPack}
        onClose={() => setStarterPacksOpen(false)}
      />

      {editorOpen && (
        <RuleEditorModal
          key={editingRule ? `edit-${editingRule.id}` : 'create'}
          open={editorOpen}
          rule={editingRule}
          busy={savingRule}
          backendError={editorError}
          onSave={handleSaveRule}
          onClose={() => setEditorOpen(false)}
        />
      )}

      <ConfirmDialog
        open={deleteTarget !== null}
        title={messages.rules.deleteDialogTitle}
        description={messages.rules.deleteDialogDesc}
        willHappen={[messages.rules.deleteWillHappen]}
        willNotHappen={[messages.rules.deleteWillNotHappen]}
        confirmLabel={messages.rules.deleteRuleAria}
        destructive
        busy={deleting}
        onConfirm={() => void handleConfirmDelete()}
        onCancel={() => setDeleteTarget(null)}
      />

      <ConfirmDialog
        open={restoreOpen}
        title={messages.rules.restoreDialogTitle}
        description={messages.rules.restoreDialogDesc}
        willHappen={[messages.rules.restoreWillHappen]}
        willNotHappen={[messages.rules.restoreWillNotHappen]}
        confirmLabel={messages.rules.restoreDefaultsButton}
        busy={restoring}
        onConfirm={() => void handleConfirmRestoreDefaults()}
        onCancel={() => setRestoreOpen(false)}
      />
    </div>
  );
}
