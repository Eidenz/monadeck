import type { ControllerProfile } from './controllers'
import { KNUCKLES_PROFILE } from './knuckles'
import { OCULUS_TOUCH_PROFILE } from './oculusTouch'
import { VIVE_WAND_PROFILE } from './viveWand'

const PROFILES: ControllerProfile[] = [
  KNUCKLES_PROFILE,
  OCULUS_TOUCH_PROFILE,
  VIVE_WAND_PROFILE,
]

/** Get profile by controller_type string. Falls back to knuckles. */
export function getProfile(controllerType: string): ControllerProfile {
  return PROFILES.find(p => p.controllerType === controllerType) || KNUCKLES_PROFILE
}

/** Get all registered profiles */
export function getAllProfiles(): ControllerProfile[] {
  return PROFILES
}

/** Get list of known controller type strings */
export function getKnownControllerTypes(): string[] {
  return PROFILES.map(p => p.controllerType)
}
