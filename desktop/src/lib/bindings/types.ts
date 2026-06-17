/**
 * SteamVR Binding JSON Schema Types
 *
 * These types model the JSON files used by SteamVR, xrizer, and OpenComposite
 * for mapping controller inputs to application actions.
 *
 * Two files work together:
 * - actions.json (ActionManifest) — defines what actions exist
 * - bindings_<controller>.json (BindingConfig) — maps inputs to actions
 */

// ─── Action Manifest (actions.json) ───────────────────────────────────────────

export type ActionType = 'boolean' | 'vector1' | 'vector2' | 'vector3' | 'vibration' | 'pose' | 'skeleton'
export type ActionRequirement = 'mandatory' | 'suggested' | 'optional'
export type ActionSetUsage = 'leftright' | 'single' | 'hidden'

export interface Action {
  name: string          // e.g. "/actions/default/in/grab"
  type: ActionType
  requirement?: ActionRequirement
  skeleton?: string     // for skeleton type actions
}

export interface ActionSet {
  name: string          // e.g. "/actions/default"
  usage: ActionSetUsage
}

export interface DefaultBinding {
  controller_type: string
  binding_url: string
}

export interface LocalizationEntry {
  language_tag: string
  [actionPath: string]: string
}

export interface ActionManifest {
  actions: Action[]
  action_sets: ActionSet[]
  default_bindings: DefaultBinding[]
  localization?: LocalizationEntry[]
}

// ─── Binding Config (bindings_<controller>.json) ──────────────────────────────

/**
 * Source modes determine how a physical input is interpreted.
 * Each mode supports different input sub-types.
 */
export type SourceMode =
  | 'button'
  | 'trigger'
  | 'joystick'
  | 'trackpad'
  | 'dpad'
  | 'scroll'
  | 'skeleton'
  | 'force_sensor'
  | 'grab'        // knuckles specific
  | 'pinch'       // knuckles specific
  | 'none'

/**
 * Input sub-types available per source mode.
 * e.g. a "button" mode has "click", "touch", "long"
 * a "trigger" mode has "pull", "click", "touch"
 */
export type InputSubType =
  | 'click'
  | 'touch'
  | 'long'
  | 'double'
  | 'pull'
  | 'value'
  | 'force'
  | 'position'
  | 'north'
  | 'south'
  | 'east'
  | 'west'
  | 'center'
  | 'scroll'

export interface InputBinding {
  output: string        // action path, e.g. "/actions/default/in/grab"
}

export interface SourceEntry {
  path: string          // physical input path, e.g. "/user/hand/left/input/trigger"
  mode: SourceMode
  inputs: Record<string, InputBinding>
  parameters?: Record<string, unknown>  // e.g. deadzone_pct, threshold, etc.
}

export interface HapticEntry {
  output: string        // action path, e.g. "/actions/default/out/haptic"
  path: string          // physical output path, e.g. "/user/hand/left/output/haptic"
}

export interface ActionSetBinding {
  sources: SourceEntry[]
  haptics?: HapticEntry[]
  chords?: unknown[]
  skeleton?: unknown[]
  poses?: unknown[]
}

export interface BindingConfig {
  app_key?: string
  bindings: Record<string, ActionSetBinding>  // keyed by action set path
  category?: string
  controller_type: string
  description: string
  name: string
  options?: Record<string, unknown>
  simulated_actions?: unknown[]
}

// ─── Editor State Types ───────────────────────────────────────────────────────

export interface EditorState {
  actionManifest: ActionManifest | null
  bindingConfig: BindingConfig | null
  activeActionSet: string | null
  selectedInput: string | null     // full path incl. hand, e.g. /user/hand/left/input/trigger
  mirrorMode: boolean
  dirty: boolean                   // unsaved changes
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/** Extract action set name from an action path */
export function getActionSetFromAction(actionPath: string): string {
  // "/actions/default/in/grab" → "/actions/default"
  const parts = actionPath.split('/')
  return parts.slice(0, 3).join('/')
}

/** Extract just the action name (last segment) */
export function getActionName(actionPath: string): string {
  return actionPath.split('/').pop() || actionPath
}

/** Extract direction (in/out) from action path */
export function getActionDirection(actionPath: string): 'in' | 'out' {
  return actionPath.includes('/out/') ? 'out' : 'in'
}

/** Get friendly display name for a physical input path */
export function getInputDisplayName(path: string): string {
  // "/user/hand/left/input/trigger" → "Left Trigger"
  const parts = path.split('/')
  const hand = parts[3] === 'left' ? 'L' : parts[3] === 'right' ? 'R' : ''
  const type = parts[4] // "input" or "output"
  const name = parts[5] || ''

  const nameMap: Record<string, string> = {
    'a': 'A Button',
    'b': 'B Button',
    'trigger': 'Trigger',
    'thumbstick': 'Thumbstick',
    'trackpad': 'Trackpad',
    'grip': 'Grip',
    'system': 'System',
    'haptic': 'Haptic',
    'finger': 'Finger',
  }

  const displayName = nameMap[name] || name
  if (type === 'output') return `${hand} ${displayName} (Output)`
  return `${hand} ${displayName}`
}

/** Get the modes available for a given input source type */
export function getAvailableModesForInput(inputType: string): SourceMode[] {
  const modeMap: Record<string, SourceMode[]> = {
    'a':          ['button', 'none'],
    'b':          ['button', 'none'],
    'trigger':    ['trigger', 'button', 'none'],
    'thumbstick': ['joystick', 'dpad', 'button', 'none'],
    'joystick':   ['joystick', 'dpad', 'button', 'none'],
    'trackpad':   ['trackpad', 'dpad', 'joystick', 'button', 'scroll', 'none'],
    'grip':       ['force_sensor', 'button', 'trigger', 'grab', 'none'],
    'system':     ['button', 'none'],
    'thumbrest':  ['button', 'none'],
  }
  return modeMap[inputType] || ['button', 'none']
}

/** Get the available input sub-types for a given source mode */
export function getInputSubTypesForMode(mode: SourceMode): InputSubType[] {
  const subTypeMap: Record<string, InputSubType[]> = {
    'button':       ['click', 'touch', 'long', 'double'],
    'trigger':      ['pull', 'click', 'touch', 'value'],
    'joystick':     ['position', 'click', 'touch'],
    'trackpad':     ['position', 'click', 'touch', 'force'],
    'dpad':         ['north', 'south', 'east', 'west', 'center'],
    'scroll':       ['scroll', 'click', 'touch'],
    'force_sensor': ['force', 'click', 'value'],
    'grab':         ['force', 'click'],
    'pinch':        ['force', 'click'],
    'skeleton':     [],
    'none':         [],
  }
  return subTypeMap[mode] || []
}
