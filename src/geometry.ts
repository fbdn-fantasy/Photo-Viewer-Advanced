import type {Crop} from './types';
/** Crop coordinates refer to original pixels after orientation, rotation and flips. */
export function rotateCrop(crop:Crop,width:number,height:number,clockwise:boolean):Crop {
 return clockwise?{x:height-crop.y-crop.height,y:crop.x,width:crop.height,height:crop.width}:{x:crop.y,y:width-crop.x-crop.width,width:crop.height,height:crop.width};
}
export function flipCrop(crop:Crop,width:number,height:number,horizontal:boolean):Crop {
 return horizontal?{...crop,x:width-crop.x-crop.width}:{...crop,y:height-crop.y-crop.height};
}
export function actualScale(devicePixelRatio:number):number {return 1/Math.max(1,devicePixelRatio);}
