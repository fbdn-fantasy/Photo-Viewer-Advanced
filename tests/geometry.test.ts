import test from 'node:test';
import assert from 'node:assert/strict';
import {rotateCrop,flipCrop,actualScale} from '../src/geometry.ts';
test('crop rotates in post-transform original pixel coordinates',()=>{const crop={x:20,y:30,width:100,height:50};assert.deepEqual(rotateCrop(crop,640,480,true),{x:400,y:20,width:50,height:100});assert.deepEqual(rotateCrop(rotateCrop(crop,640,480,true),480,640,false),crop);});
test('four quarter turns and double flips restore crop exactly',()=>{const original={x:31,y:17,width:107,height:59};let crop=original,width=640,height=480;for(let i=0;i<4;i++){crop=rotateCrop(crop,width,height,true);[width,height]=[height,width];}assert.deepEqual(crop,original);for(const horizontal of [true,false])assert.deepEqual(flipCrop(flipCrop(original,width,height,horizontal),width,height,horizontal),original);});
test('100 percent is one source pixel to one physical display pixel',()=>{for(const dpr of [1,1.25,1.5,2,3])assert.equal(actualScale(dpr)*dpr,1);});
