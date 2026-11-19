/// <reference lib="webworker" />
export {};
// ImageDecoder performs native disposal, compositing, color conversion and EXIF orientation.
// Only requested frames cross this boundary; the main thread holds at most two decoded frames.
declare class ImageDecoder {
  constructor(init:{data:ArrayBuffer;type:string;preferAnimation:boolean; colorSpaceConversion:'default'});
  static isTypeSupported(type:string):Promise<boolean>;
  tracks:{ready:Promise<void>;selectedTrack:{frameCount:number;repetitionCount:number;animated:boolean}|null};
  completed:Promise<void>;
  decode(options:{frameIndex:number;completeFramesOnly:boolean}):Promise<{image:VideoFrame;complete:boolean}>;
  close():void;
}
let decoder:ImageDecoder|null = null;
let generation=0;
let bytes:ArrayBuffer|null=null;
let mime='';
let frames=1;
let chain=Promise.resolve();
async function makeDecoder() {decoder?.close();decoder=new ImageDecoder({data:bytes!,type:mime,preferAnimation:true,colorSpaceConversion:'default'});await decoder.tracks.ready;await decoder.completed;}
self.onmessage = (event:MessageEvent) => {
  const message=event.data;
  if(message.type==='close'){generation++;decoder?.close();decoder=null;bytes=null;return;}
  if(message.type==='open'){
    const current=++generation;
    chain=(async()=>{
      try {
        mime=message.mime;
        if(!('ImageDecoder' in self) || !(await ImageDecoder.isTypeSupported(mime))) throw new Error('This WebView2 runtime does not support controlled playback of '+mime+'.');
        const response=await fetch(message.url);if(!response.ok)throw new Error('The image could not be read.');
        bytes=await response.arrayBuffer();if(current!==generation)return;
        await makeDecoder();if(current!==generation)return;
        const track=decoder!.tracks.selectedTrack!;frames=track.frameCount;
        self.postMessage({type:'ready',frameCount:frames,repetitionCount:track.repetitionCount,animated:track.animated});
      } catch(error){if(current===generation)self.postMessage({type:'error',message:String(error)});}
    })();
  }
  if(message.type==='frame'){
    const current=generation;
    chain=chain.then(async()=>{
      if(current!==generation||!decoder)return;
      try{
        // Reopen between loops to release the native decoder's previous loop state.
        if(message.reset)await makeDecoder();
        const result=await decoder!.decode({frameIndex:message.index,completeFramesOnly:true});
        const duration=result.image.duration;const bitmap=await createImageBitmap(result.image);result.image.close();
        if(current!==generation){bitmap.close();return;}
        self.postMessage({type:'frame',bitmap,duration,index:message.index},[bitmap]);
      }catch(error){if(current===generation)self.postMessage({type:'error',message:String(error)});}
    });
  }
};
