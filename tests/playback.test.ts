import test from 'node:test';
import assert from 'node:assert/strict';
import {shouldRepeat,frameDelay,clamp} from '../src/playback.ts';
test('GIF/WebP native finite repetitions mean additional plays',()=>{assert.equal(shouldRepeat('original',0,1),false);assert.equal(shouldRepeat('original',2,1),true);assert.equal(shouldRepeat('original',2,2),true);assert.equal(shouldRepeat('original',2,3),false);});
test('infinite loops and explicit overrides',()=>{assert.equal(shouldRepeat('original',Infinity,10000),true);assert.equal(shouldRepeat('once',Infinity,1),false);assert.equal(shouldRepeat('forever',0,10000),true);});
test('preserves varied positive frame delays including sub-20ms frames',()=>{assert.equal(frameDelay(10000),10);assert.equal(frameDelay(8333),8.333);assert.equal(frameDelay(250000),250);assert.equal(frameDelay(null),100);assert.equal(frameDelay(0),100);});
test('zoom constraints do not invert limits',()=>{assert.equal(clamp(0.0001,0.01,32),0.01);assert.equal(clamp(100,0.01,32),32);});
