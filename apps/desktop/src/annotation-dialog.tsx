import { useState, type FormEvent } from "react";
import {
  CONTEXT_TAG_LABELS,
  contextTagOptions,
  toggleContextTag,
} from "./check-in";
import {
  createSaveAnnotationRequest,
  emptyAnnotationDraft,
  updateAnnotationDraftTextField,
  type AnnotationDraft,
} from "./annotation";
import type { AnnotationRecord } from "@mindcanary/protocol";
import { Dialog } from "./dialog";

export function AnnotationDialog({
  initialDraft,
  onClose,
  onSave,
}: {
  initialDraft?: AnnotationDraft;
  onClose: () => void;
  onSave: (annotation: AnnotationRecord) => Promise<void>;
}) {
  const [draft, setDraft] = useState(initialDraft ?? emptyAnnotationDraft());
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string>();
  const isEditing = draft.annotationId !== undefined;

  function update(
    field: "localDate" | "startTime" | "endTime" | "text",
    value: string,
  ): void {
    setDraft((current) =>
      updateAnnotationDraftTextField(current, field, value),
    );
    setError(undefined);
  }

  async function save(e: FormEvent) {
    e.preventDefault();
    setError(undefined);
    setSaving(true);
    try {
      const request = createSaveAnnotationRequest(draft);
      if (request.type !== "save_annotation") {
        throw new TypeError("Unexpected annotation request.");
      }
      await onSave(request.annotation);
    } catch (caught) {
      setError(
        caught instanceof Error ? caught.message : "Could not save annotation.",
      );
      setSaving(false);
    }
  }

  return (
    <Dialog
      eyebrow={isEditing ? "Edit private note" : "Private note"}
      title={isEditing ? "Update this context" : "Add context to a day"}
      onClose={onClose}
      wide
    >
      <form onSubmit={save} className="annotation-form">
        <div className="annotation-field-grid">
          <label className="field">
            <span>Local date</span>
            <input
              type="text"
              inputMode="numeric"
              pattern="\d{4}-\d{2}-\d{2}"
              placeholder="YYYY-MM-DD"
              value={draft.localDate}
              onChange={(event) => update("localDate", event.target.value)}
              required
            />
          </label>
          <label className="field">
            <span>Start (optional)</span>
            <input
              type="text"
              pattern="\d{2}:\d{2}"
              placeholder="13:30"
              value={draft.startTime}
              onChange={(event) => update("startTime", event.target.value)}
            />
          </label>
          <label className="field">
            <span>End (optional)</span>
            <input
              type="text"
              pattern="\d{2}:\d{2}"
              placeholder="14:45"
              value={draft.endTime}
              onChange={(event) => update("endTime", event.target.value)}
            />
          </label>
        </div>

        <label className="field">
          <span>What was happening?</span>
          <textarea
            value={draft.text}
            onChange={(event) => update("text", event.target.value)}
            placeholder="Travel, a deadline, an unusual schedule, or anything else that felt relevant."
            rows={4}
            maxLength={1000}
          />
        </label>

        <fieldset className="context-fieldset">
          <legend>Context tags (optional)</legend>
          <div className="tag-list">
            {contextTagOptions().map((tag) => {
              const selected = draft.contextTags.includes(tag);
              return (
                <button
                  type="button"
                  key={tag}
                  className="tag-button"
                  data-selected={selected}
                  aria-pressed={selected}
                  onClick={() => {
                    try {
                      setDraft((d) => ({
                        ...d,
                        contextTags: toggleContextTag(d.contextTags, tag),
                      }));
                      setError(undefined);
                    } catch (caught) {
                      setError(
                        caught instanceof Error
                          ? caught.message
                          : "Too many tags.",
                      );
                    }
                  }}
                >
                  {CONTEXT_TAG_LABELS[tag]}
                </button>
              );
            })}
          </div>
        </fieldset>

        {error && (
          <p className="form-error" role="alert">
            {error}
          </p>
        )}

        <div className="dialog-actions">
          <span className="form-hint">
            Stored in the encrypted local database.
          </span>
          <button
            type="button"
            className="secondary-button"
            disabled={saving}
            onClick={onClose}
          >
            Cancel
          </button>
          <button
            type="submit"
            className="primary-button"
            disabled={saving || draft.text.trim().length === 0}
          >
            {saving ? "Saving..." : isEditing ? "Update note" : "Save note"}
          </button>
        </div>
      </form>
    </Dialog>
  );
}
