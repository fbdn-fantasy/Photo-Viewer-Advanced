export interface ImageInfo {path:string;displayPath:string;name:string;width:number;height:number;format:string;animated:boolean;animationSupported:boolean;warning:string|null;canSaveOriginal:boolean}
export interface Entry {path:string;name:string}
export interface Settings {theme:'system'|'dark'|'light';defaultZoom:'fit'|'actual';autoplay:boolean;loop:'original'|'forever'|'once';slideshowInterval:number;showThumbnails:boolean}
export interface Crop {x:number;y:number;width:number;height:number}
export interface SaveRequest {path:string;destination:string;overwrite:boolean;rotate:number;flipHorizontal:boolean;flipVertical:boolean;crop?:Crop;frameIndex?:number;framePng?:number[]}
export const defaultSettings:Settings = {theme:'system',defaultZoom:'fit',autoplay:true,loop:'original',slideshowInterval:5,showThumbnails:false};
