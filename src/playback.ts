/** WebCodecs repetitionCount is repeats AFTER the first presentation. */
export function shouldRepeat(mode:'original'|'forever'|'once', repeats:number, completedPlays:number):boolean {
  if (mode === 'forever') return true;
  if (mode === 'once') return false;
  return repeats === Infinity || completedPlays <= Math.max(0,repeats);
}
/** Frame durations are microseconds. Preserve positive source timing; use 100 ms only when absent. */
export function frameDelay(duration:number|null|undefined):number {return duration != null && duration > 0 ? duration/1000 : 100;}
export function clamp(value:number,min:number,max:number):number {return Math.min(max,Math.max(min,value));}
