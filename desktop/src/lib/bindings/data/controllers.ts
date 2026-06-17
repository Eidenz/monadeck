import type { SourceMode } from '../types'

export interface ControllerInput {
  id: string
  label: string
  shortLabel: string
  pathSuffix: string
  type: 'button' | 'trigger' | 'thumbstick' | 'trackpad' | 'grip' | 'system' | 'joystick' | 'thumbrest'
  defaultModes: SourceMode[]
  // SVG center position for the RIGHT-hand diagram
  cx: number
  cy: number
  // Hit-area radius
  r: number
  shape: 'circle' | 'rect' | 'pill'
  // Optional rect/pill dimensions
  w?: number
  h?: number
  // Optional rotation in degrees (around cx, cy)
  rotation?: number
  // Hide the label below the hit zone on the diagram
  hideLabel?: boolean
  // Side-specific input (only appears on this hand)
  side?: 'left' | 'right' | 'both'
  // Mirror input ID (e.g., 'x' on left mirrors to 'a' on right)
  mirrorOf?: string
}

export interface ControllerProfile {
  name: string
  controllerType: string
  displayName: string
  inputs: ControllerInput[]
  hapticPath: string
  svgViewBox: string
  svgAsset: string
  // Mirror mappings: left input suffix -> right input suffix
  mirrorMappings?: [string, string][]
}

export function buildInputPath(hand: 'left' | 'right', pathSuffix: string): string {
  return `/user/hand/${hand}/${pathSuffix}`
}

export function getControllerInputFromPath(profile: ControllerProfile, path: string): ControllerInput | null {
  for (const input of profile.inputs) {
    if (path.endsWith(input.pathSuffix)) return input
  }
  return null
}

/** Get inputs visible for a given hand (respects side-specific buttons) */
export function getInputsForHand(profile: ControllerProfile, hand: 'left' | 'right'): ControllerInput[] {
  return profile.inputs.filter(input => {
    if (!input.side || input.side === 'both') return true
    return input.side === hand
  })
}

/** Get the mirror path for a given input path using profile's mirror mappings */
export function getMirrorPath(profile: ControllerProfile, path: string): string | null {
  if (!profile.mirrorMappings) {
    // Default: just swap hand
    if (path.includes('/hand/left/')) return path.replace('/hand/left/', '/hand/right/')
    if (path.includes('/hand/right/')) return path.replace('/hand/right/', '/hand/left/')
    return null
  }

  // Check mirror mappings for side-specific buttons
  for (const [leftSuffix, rightSuffix] of profile.mirrorMappings) {
    if (path.endsWith(leftSuffix.replace('/user/hand/left/', ''))) {
      const hand = path.includes('/hand/left/') ? 'right' : 'left'
      const suffix = hand === 'right' ? rightSuffix.replace('/user/hand/right/', '') : leftSuffix.replace('/user/hand/left/', '')
      return `/user/hand/${hand}/${suffix}`
    }
    if (path.endsWith(rightSuffix.replace('/user/hand/right/', ''))) {
      const hand = path.includes('/hand/right/') ? 'left' : 'right'
      const suffix = hand === 'left' ? leftSuffix.replace('/user/hand/left/', '') : rightSuffix.replace('/user/hand/right/', '')
      return `/user/hand/${hand}/${suffix}`
    }
  }

  // Fallback: just swap hand
  if (path.includes('/hand/left/')) return path.replace('/hand/left/', '/hand/right/')
  if (path.includes('/hand/right/')) return path.replace('/hand/right/', '/hand/left/')
  return null
}
