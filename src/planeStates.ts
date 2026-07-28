export const CONTROL_PLANE_STATES = [
  "cold",
  "decrypting",
  "starting",
  "ready",
  "degraded",
  "failed",
  "stopping",
] as const;

export const DATA_PLANE_STATES = [
  "unconfigured",
  "validating",
  "permission_required",
  "starting",
  "online",
  "stopping",
  "failed",
  "rollback",
] as const;

export type ControlPlaneState = (typeof CONTROL_PLANE_STATES)[number];
export type DataPlaneState = (typeof DATA_PLANE_STATES)[number];
