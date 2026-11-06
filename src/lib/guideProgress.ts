/** Progression du parcours guidé — stockée localement par workspace. */

export const GUIDE_STEPS = [
  "societe",
  "plan",
  "ecritures",
  "tva",
  "bilan",
] as const;

export type GuideStepId = (typeof GUIDE_STEPS)[number];

export type GuideProgress = {
  completed: GuideStepId[];
  dismissedReminders: string[];
  wizardDismissed?: boolean;
};

const PREFIX = "sascompta.guide.";

function key(workspacePath: string): string {
  return PREFIX + encodeURIComponent(workspacePath);
}

export function loadGuideProgress(workspacePath: string): GuideProgress {
  try {
    const raw = localStorage.getItem(key(workspacePath));
    if (!raw) return { completed: [], dismissedReminders: [] };
    const parsed = JSON.parse(raw) as GuideProgress;
    return {
      completed: Array.isArray(parsed.completed) ? parsed.completed : [],
      dismissedReminders: Array.isArray(parsed.dismissedReminders)
        ? parsed.dismissedReminders
        : [],
      wizardDismissed: Boolean(parsed.wizardDismissed),
    };
  } catch {
    return { completed: [], dismissedReminders: [] };
  }
}

export function saveGuideProgress(
  workspacePath: string,
  progress: GuideProgress,
): void {
  localStorage.setItem(key(workspacePath), JSON.stringify(progress));
}

export function markStepDone(
  workspacePath: string,
  step: GuideStepId,
): GuideProgress {
  const current = loadGuideProgress(workspacePath);
  if (!current.completed.includes(step)) {
    current.completed = [...current.completed, step];
  }
  saveGuideProgress(workspacePath, current);
  return current;
}

export function dismissReminder(
  workspacePath: string,
  reminderId: string,
): GuideProgress {
  const current = loadGuideProgress(workspacePath);
  if (!current.dismissedReminders.includes(reminderId)) {
    current.dismissedReminders = [...current.dismissedReminders, reminderId];
  }
  saveGuideProgress(workspacePath, current);
  return current;
}

export function stepIndex(id: GuideStepId): number {
  return GUIDE_STEPS.indexOf(id);
}
