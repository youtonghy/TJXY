"use strict";
import { ProgressCircleRoot, ProgressCircleFillCircle, ProgressCircleTrackCircle, ProgressCircleTrack } from './progress-circle.js';
export { progressCircleVariants } from '@heroui/styles';

/* -------------------------------------------------------------------------------------------------
 * Compound Component
 * -----------------------------------------------------------------------------------------------*/
const ProgressCircle = Object.assign(ProgressCircleRoot, {
  Root: ProgressCircleRoot,
  Track: ProgressCircleTrack,
  TrackCircle: ProgressCircleTrackCircle,
  FillCircle: ProgressCircleFillCircle
});

export { ProgressCircle, ProgressCircleFillCircle, ProgressCircleRoot, ProgressCircleTrack, ProgressCircleTrackCircle };
