// pkg/mb_host.js
function host_begin_frame() {
  wasm.host_begin_frame();
}
function host_draw_hash() {
  let deferred1_0;
  let deferred1_1;
  try {
    const ret = wasm.host_draw_hash();
    deferred1_0 = ret[0];
    deferred1_1 = ret[1];
    return getStringFromWasm0(ret[0], ret[1]);
  } finally {
    wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
  }
}
function host_end_frame() {
  const ret = wasm.host_end_frame();
  return ret !== 0;
}
function host_frame_time(ms) {
  wasm.host_frame_time(ms);
}
function host_gpu_connect() {
  const ret = wasm.host_gpu_connect();
  return ret;
}
function host_gpu_debug_lose() {
  wasm.host_gpu_debug_lose();
}
function host_gpu_done() {
  const ret = wasm.host_gpu_done();
  return ret;
}
function host_gpu_fault(message2) {
  const ptr0 = passStringToWasm0(message2, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
  const len0 = WASM_VECTOR_LEN;
  wasm.host_gpu_fault(ptr0, len0);
}
function host_gpu_install() {
  const ret = wasm.host_gpu_install();
  return ret !== 0;
}
function host_gpu_log() {
  const ret = wasm.host_gpu_log();
  var v1 = getArrayJsValueFromWasm0(ret[0], ret[1]);
  wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
  return v1;
}
function host_info() {
  let deferred1_0;
  let deferred1_1;
  try {
    const ret = wasm.host_info();
    deferred1_0 = ret[0];
    deferred1_1 = ret[1];
    return getStringFromWasm0(ret[0], ret[1]);
  } finally {
    wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
  }
}
function host_init(canvas, logical_w, logical_h) {
  const ret = wasm.host_init(canvas, logical_w, logical_h);
  return ret;
}
function host_new_session() {
  wasm.host_new_session();
}
function host_pose() {
  const ret = wasm.host_pose();
  var v1 = getArrayF32FromWasm0(ret[0], ret[1]).slice();
  wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
  return v1;
}
function host_prepare() {
  const ret = wasm.host_prepare();
  return ret !== 0;
}
function host_redraw() {
  const ret = wasm.host_redraw();
  return ret !== 0;
}
function host_redraw_isolated(node, hud, plain) {
  const ret = wasm.host_redraw_isolated(node, hud, plain);
  return ret !== 0;
}
function host_resize(width, height) {
  wasm.host_resize(width, height);
}
function host_set_logical(width, height) {
  wasm.host_set_logical(width, height);
}
function host_set_stdlib(mb3d) {
  wasm.host_set_stdlib(mb3d);
}
function host_stats() {
  let deferred1_0;
  let deferred1_1;
  try {
    const ret = wasm.host_stats();
    deferred1_0 = ret[0];
    deferred1_1 = ret[1];
    return getStringFromWasm0(ret[0], ret[1]);
  } finally {
    wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
  }
}
function host_step(dt) {
  wasm.host_step(dt);
}
function host_take_refusal() {
  let deferred1_0;
  let deferred1_1;
  try {
    const ret = wasm.host_take_refusal();
    deferred1_0 = ret[0];
    deferred1_1 = ret[1];
    return getStringFromWasm0(ret[0], ret[1]);
  } finally {
    wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
  }
}
function host_top_draws(n) {
  let deferred1_0;
  let deferred1_1;
  try {
    const ret = wasm.host_top_draws(n);
    deferred1_0 = ret[0];
    deferred1_1 = ret[1];
    return getStringFromWasm0(ret[0], ret[1]);
  } finally {
    wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
  }
}
function host_track(node) {
  wasm.host_track(node);
}
function __wbg_get_imports() {
  const import0 = {
    __proto__: null,
    __wbg_Window_a2a6c4d665047b14: function(arg0) {
      const ret = arg0.Window;
      return ret;
    },
    __wbg_WorkerGlobalScope_2664448a7c667d67: function(arg0) {
      const ret = arg0.WorkerGlobalScope;
      return ret;
    },
    __wbg___wbindgen_debug_string_4687d8d8c2017d52: function(arg0, arg1) {
      const ret = debugString(arg1);
      const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
      const len1 = WASM_VECTOR_LEN;
      getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
      getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
    },
    __wbg___wbindgen_is_function_1f9d30630b8b1d3d: function(arg0) {
      const ret = typeof arg0 === "function";
      return ret;
    },
    __wbg___wbindgen_is_null_e343b7d08827ba72: function(arg0) {
      const ret = arg0 === null;
      return ret;
    },
    __wbg___wbindgen_is_string_90b56bc79aad6f6c: function(arg0) {
      const ret = typeof arg0 === "string";
      return ret;
    },
    __wbg___wbindgen_is_undefined_8865fb403f8fe9d8: function(arg0) {
      const ret = arg0 === void 0;
      return ret;
    },
    __wbg___wbindgen_throw_41e9ee4f547fc59a: function(arg0, arg1) {
      throw new Error(getStringFromWasm0(arg0, arg1));
    },
    __wbg__wbg_cb_unref_dcc1a90847f04c41: function(arg0) {
      arg0._wbg_cb_unref();
    },
    __wbg_beginRenderPass_3c53642423af50dc: function() {
      return handleError(function(arg0, arg1) {
        const ret = arg0.beginRenderPass(arg1);
        return ret;
      }, arguments);
    },
    __wbg_call_187d372bd5fdd4aa: function() {
      return handleError(function(arg0, arg1, arg2) {
        const ret = arg0.call(arg1, arg2);
        return ret;
      }, arguments);
    },
    __wbg_configure_1e2c1c9edad07d26: function() {
      return handleError(function(arg0, arg1) {
        arg0.configure(arg1);
      }, arguments);
    },
    __wbg_createBindGroupLayout_b1bd63b4e88459d8: function() {
      return handleError(function(arg0, arg1) {
        const ret = arg0.createBindGroupLayout(arg1);
        return ret;
      }, arguments);
    },
    __wbg_createBindGroup_f539b26ca341308f: function(arg0, arg1) {
      const ret = arg0.createBindGroup(arg1);
      return ret;
    },
    __wbg_createBuffer_d800e9b1d41b2ee5: function() {
      return handleError(function(arg0, arg1) {
        const ret = arg0.createBuffer(arg1);
        return ret;
      }, arguments);
    },
    __wbg_createCommandEncoder_3352d1ffc36c6fc0: function(arg0, arg1) {
      const ret = arg0.createCommandEncoder(arg1);
      return ret;
    },
    __wbg_createPipelineLayout_6eab52c327118937: function(arg0, arg1) {
      const ret = arg0.createPipelineLayout(arg1);
      return ret;
    },
    __wbg_createRenderPipeline_0ebb7ebc653e9207: function() {
      return handleError(function(arg0, arg1) {
        const ret = arg0.createRenderPipeline(arg1);
        return ret;
      }, arguments);
    },
    __wbg_createSampler_9bd91d7e928c0060: function(arg0, arg1) {
      const ret = arg0.createSampler(arg1);
      return ret;
    },
    __wbg_createShaderModule_cefa51336cb288ae: function(arg0, arg1) {
      const ret = arg0.createShaderModule(arg1);
      return ret;
    },
    __wbg_createTexture_ed7e9fc04dd54d84: function() {
      return handleError(function(arg0, arg1) {
        const ret = arg0.createTexture(arg1);
        return ret;
      }, arguments);
    },
    __wbg_createView_da41c2d2cb212715: function() {
      return handleError(function(arg0, arg1) {
        const ret = arg0.createView(arg1);
        return ret;
      }, arguments);
    },
    __wbg_description_83b8a393160021b9: function(arg0, arg1) {
      const ret = arg1.description;
      const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
      const len1 = WASM_VECTOR_LEN;
      getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
      getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
    },
    __wbg_destroy_275564a0a8b8d071: function(arg0) {
      arg0.destroy();
    },
    __wbg_document_9854e03c05fc8834: function(arg0) {
      const ret = arg0.document;
      return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
    },
    __wbg_drawIndexed_638959aae942557c: function(arg0, arg1, arg2, arg3, arg4, arg5) {
      arg0.drawIndexed(arg1 >>> 0, arg2 >>> 0, arg3 >>> 0, arg4, arg5 >>> 0);
    },
    __wbg_draw_086a9578fc9898c2: function(arg0, arg1, arg2, arg3, arg4) {
      arg0.draw(arg1 >>> 0, arg2 >>> 0, arg3 >>> 0, arg4 >>> 0);
    },
    __wbg_end_b57473834b877409: function(arg0) {
      arg0.end();
    },
    __wbg_error_8c8dff942c7c4f11: function(arg0) {
      const ret = arg0.error;
      return ret;
    },
    __wbg_finish_09ec094c10f41e7b: function(arg0) {
      const ret = arg0.finish();
      return ret;
    },
    __wbg_finish_ec1c191f66a895b1: function(arg0, arg1) {
      const ret = arg0.finish(arg1);
      return ret;
    },
    __wbg_getContext_635e36719cad2623: function() {
      return handleError(function(arg0, arg1, arg2) {
        const ret = arg0.getContext(getStringFromWasm0(arg1, arg2));
        return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
      }, arguments);
    },
    __wbg_getContext_e567868594d2e0c6: function() {
      return handleError(function(arg0, arg1, arg2) {
        const ret = arg0.getContext(getStringFromWasm0(arg1, arg2));
        return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
      }, arguments);
    },
    __wbg_getCurrentTexture_9f3b84d0eaa6cd95: function() {
      return handleError(function(arg0) {
        const ret = arg0.getCurrentTexture();
        return ret;
      }, arguments);
    },
    __wbg_getMappedRange_fb54c6327b2d8d20: function() {
      return handleError(function(arg0, arg1, arg2) {
        const ret = arg0.getMappedRange(arg1, arg2);
        return ret;
      }, arguments);
    },
    __wbg_getPreferredCanvasFormat_0ef5034c8902201b: function(arg0) {
      const ret = arg0.getPreferredCanvasFormat();
      return (__wbindgen_enum_GpuTextureFormat.indexOf(ret) + 1 || 102) - 1;
    },
    __wbg_get_fd12a9100976c296: function(arg0, arg1) {
      const ret = arg0[arg1 >>> 0];
      return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
    },
    __wbg_gpu_afdd4387c7afe5f9: function(arg0) {
      const ret = arg0.gpu;
      return ret;
    },
    __wbg_height_aa756cf29b6d629c: function(arg0) {
      const ret = arg0.height;
      return ret;
    },
    __wbg_height_fc97e1a0c2e7331f: function(arg0) {
      const ret = arg0.height;
      return ret;
    },
    __wbg_info_971d8b9db3dae69f: function(arg0) {
      const ret = arg0.info;
      return ret;
    },
    __wbg_instanceof_GpuOutOfMemoryError_5c3b8a2499f59adb: function(arg0) {
      let result;
      try {
        result = arg0 instanceof GPUOutOfMemoryError;
      } catch (_) {
        result = false;
      }
      const ret = result;
      return ret;
    },
    __wbg_instanceof_GpuValidationError_7659e07b12d4e184: function(arg0) {
      let result;
      try {
        result = arg0 instanceof GPUValidationError;
      } catch (_) {
        result = false;
      }
      const ret = result;
      return ret;
    },
    __wbg_instanceof_Window_82d71df4eddf88bc: function(arg0) {
      let result;
      try {
        result = arg0 instanceof Window;
      } catch (_) {
        result = false;
      }
      const ret = result;
      return ret;
    },
    __wbg_isFallbackAdapter_4c8cc3b18677460a: function(arg0) {
      const ret = arg0.isFallbackAdapter;
      return ret;
    },
    __wbg_label_7add8cb37a6ef98f: function(arg0, arg1) {
      const ret = arg1.label;
      const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
      const len1 = WASM_VECTOR_LEN;
      getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
      getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
    },
    __wbg_length_7f3c00c40364105e: function(arg0) {
      const ret = arg0.length;
      return ret;
    },
    __wbg_limits_601ad2e086ef8141: function(arg0) {
      const ret = arg0.limits;
      return ret;
    },
    __wbg_lost_bb454eb62101af1a: function(arg0) {
      const ret = arg0.lost;
      return ret;
    },
    __wbg_mapAsync_b0597127f5037286: function(arg0, arg1, arg2, arg3) {
      const ret = arg0.mapAsync(arg1 >>> 0, arg2, arg3);
      return ret;
    },
    __wbg_maxBindGroupsPlusVertexBuffers_52369f089736ef9d: function(arg0) {
      const ret = arg0.maxBindGroupsPlusVertexBuffers;
      return ret;
    },
    __wbg_maxBindGroups_4e424afe6ce86ca2: function(arg0) {
      const ret = arg0.maxBindGroups;
      return ret;
    },
    __wbg_maxBindingsPerBindGroup_7d035da36821c44f: function(arg0) {
      const ret = arg0.maxBindingsPerBindGroup;
      return ret;
    },
    __wbg_maxBufferSize_423f4a084e32a195: function(arg0) {
      const ret = arg0.maxBufferSize;
      return ret;
    },
    __wbg_maxColorAttachmentBytesPerSample_c4cd9126f6d287c6: function(arg0) {
      const ret = arg0.maxColorAttachmentBytesPerSample;
      return ret;
    },
    __wbg_maxColorAttachments_d924670762b9e250: function(arg0) {
      const ret = arg0.maxColorAttachments;
      return ret;
    },
    __wbg_maxComputeInvocationsPerWorkgroup_707a3868f7cebb59: function(arg0) {
      const ret = arg0.maxComputeInvocationsPerWorkgroup;
      return ret;
    },
    __wbg_maxComputeWorkgroupSizeX_0a4d99463cbd6e5e: function(arg0) {
      const ret = arg0.maxComputeWorkgroupSizeX;
      return ret;
    },
    __wbg_maxComputeWorkgroupSizeY_85123ea0587f7558: function(arg0) {
      const ret = arg0.maxComputeWorkgroupSizeY;
      return ret;
    },
    __wbg_maxComputeWorkgroupSizeZ_a3186b4c5267d44f: function(arg0) {
      const ret = arg0.maxComputeWorkgroupSizeZ;
      return ret;
    },
    __wbg_maxComputeWorkgroupStorageSize_57b297355cfb6204: function(arg0) {
      const ret = arg0.maxComputeWorkgroupStorageSize;
      return ret;
    },
    __wbg_maxComputeWorkgroupsPerDimension_4158f95e673d54c4: function(arg0) {
      const ret = arg0.maxComputeWorkgroupsPerDimension;
      return ret;
    },
    __wbg_maxDynamicStorageBuffersPerPipelineLayout_226b0b70910aa16c: function(arg0) {
      const ret = arg0.maxDynamicStorageBuffersPerPipelineLayout;
      return ret;
    },
    __wbg_maxDynamicUniformBuffersPerPipelineLayout_0e835fda711fc7e6: function(arg0) {
      const ret = arg0.maxDynamicUniformBuffersPerPipelineLayout;
      return ret;
    },
    __wbg_maxInterStageShaderVariables_8c4a1d727e2aa35a: function(arg0) {
      const ret = arg0.maxInterStageShaderVariables;
      return ret;
    },
    __wbg_maxSampledTexturesPerShaderStage_6675f5e91d9a728a: function(arg0) {
      const ret = arg0.maxSampledTexturesPerShaderStage;
      return ret;
    },
    __wbg_maxSamplersPerShaderStage_1910fa38a6ed1e1f: function(arg0) {
      const ret = arg0.maxSamplersPerShaderStage;
      return ret;
    },
    __wbg_maxStorageBufferBindingSize_2e244bded070b18d: function(arg0) {
      const ret = arg0.maxStorageBufferBindingSize;
      return ret;
    },
    __wbg_maxStorageBuffersPerShaderStage_a285f3ebca51ca0d: function(arg0) {
      const ret = arg0.maxStorageBuffersPerShaderStage;
      return ret;
    },
    __wbg_maxStorageTexturesPerShaderStage_7aa946f0fc322a2b: function(arg0) {
      const ret = arg0.maxStorageTexturesPerShaderStage;
      return ret;
    },
    __wbg_maxTextureArrayLayers_0e699147ad00502d: function(arg0) {
      const ret = arg0.maxTextureArrayLayers;
      return ret;
    },
    __wbg_maxTextureDimension1D_aabf6add54decfe2: function(arg0) {
      const ret = arg0.maxTextureDimension1D;
      return ret;
    },
    __wbg_maxTextureDimension2D_dd598b27e9c0c1c4: function(arg0) {
      const ret = arg0.maxTextureDimension2D;
      return ret;
    },
    __wbg_maxTextureDimension3D_f944266c65dfd1a9: function(arg0) {
      const ret = arg0.maxTextureDimension3D;
      return ret;
    },
    __wbg_maxUniformBufferBindingSize_59fa6be7cfbeeb53: function(arg0) {
      const ret = arg0.maxUniformBufferBindingSize;
      return ret;
    },
    __wbg_maxUniformBuffersPerShaderStage_bee5f00a4d706c7f: function(arg0) {
      const ret = arg0.maxUniformBuffersPerShaderStage;
      return ret;
    },
    __wbg_maxVertexAttributes_5cf6392c4e9033fe: function(arg0) {
      const ret = arg0.maxVertexAttributes;
      return ret;
    },
    __wbg_maxVertexBufferArrayStride_548baa887375d865: function(arg0) {
      const ret = arg0.maxVertexBufferArrayStride;
      return ret;
    },
    __wbg_maxVertexBuffers_75d881156591f5da: function(arg0) {
      const ret = arg0.maxVertexBuffers;
      return ret;
    },
    __wbg_message_2aad50368e15e8f1: function(arg0, arg1) {
      const ret = arg1.message;
      const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
      const len1 = WASM_VECTOR_LEN;
      getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
      getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
    },
    __wbg_message_ab4cad9eeb386607: function(arg0, arg1) {
      const ret = arg1.message;
      const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
      const len1 = WASM_VECTOR_LEN;
      getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
      getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
    },
    __wbg_minStorageBufferOffsetAlignment_5ba9b77792bdadb3: function(arg0) {
      const ret = arg0.minStorageBufferOffsetAlignment;
      return ret;
    },
    __wbg_minUniformBufferOffsetAlignment_ab7d52a5293b22bd: function(arg0) {
      const ret = arg0.minUniformBufferOffsetAlignment;
      return ret;
    },
    __wbg_navigator_2156486643462a87: function(arg0) {
      const ret = arg0.navigator;
      return ret;
    },
    __wbg_navigator_c9bce48d4b578c9c: function(arg0) {
      const ret = arg0.navigator;
      return ret;
    },
    __wbg_new_617a8cdb8bb1130e: function() {
      const ret = new Object();
      return ret;
    },
    __wbg_new_a217df4351db4b9b: function() {
      return handleError(function(arg0, arg1) {
        const ret = new OffscreenCanvas(arg0 >>> 0, arg1 >>> 0);
        return ret;
      }, arguments);
    },
    __wbg_new_typed_689a3a281a8d4da6: function() {
      const ret = new Object();
      return ret;
    },
    __wbg_new_typed_b01cb72a8af741a3: function(arg0, arg1) {
      try {
        var state0 = { a: arg0, b: arg1 };
        var cb0 = (arg02, arg12) => {
          const a = state0.a;
          state0.a = 0;
          try {
            return wasm_bindgen_265b8cf50d59c234___convert__closures_____invoke___js_sys_1be3539c672eb81b___Function_fn_wasm_bindgen_265b8cf50d59c234___JsValue_____wasm_bindgen_265b8cf50d59c234___sys__Undefined___js_sys_1be3539c672eb81b___Function_fn_wasm_bindgen_265b8cf50d59c234___JsValue_____wasm_bindgen_265b8cf50d59c234___sys__Undefined_______true_(a, state0.b, arg02, arg12);
          } finally {
            state0.a = a;
          }
        };
        const ret = new Promise(cb0);
        return ret;
      } finally {
        state0.a = 0;
      }
    },
    __wbg_new_with_byte_offset_and_length_2f5d7fc2a828b74d: function(arg0, arg1, arg2) {
      const ret = new Uint8Array(arg0, arg1 >>> 0, arg2 >>> 0);
      return ret;
    },
    __wbg_onSubmittedWorkDone_1190213cee1ecf7e: function(arg0) {
      const ret = arg0.onSubmittedWorkDone();
      return ret;
    },
    __wbg_prototypesetcall_bc27214492979395: function(arg0, arg1, arg2) {
      Uint8Array.prototype.set.call(getArrayU8FromWasm0(arg0, arg1), arg2);
    },
    __wbg_querySelectorAll_6aebfe3df1fb2014: function() {
      return handleError(function(arg0, arg1, arg2) {
        const ret = arg0.querySelectorAll(getStringFromWasm0(arg1, arg2));
        return ret;
      }, arguments);
    },
    __wbg_queueMicrotask_9833f9a49df95a49: function(arg0) {
      const ret = arg0.queueMicrotask;
      return ret;
    },
    __wbg_queueMicrotask_a72f977e97f23c5f: function(arg0) {
      queueMicrotask(arg0);
    },
    __wbg_queue_7b62c28143d44293: function(arg0) {
      const ret = arg0.queue;
      return ret;
    },
    __wbg_reason_b4fa3af867fc4430: function(arg0) {
      const ret = arg0.reason;
      return (__wbindgen_enum_GpuDeviceLostReason.indexOf(ret) + 1 || 3) - 1;
    },
    __wbg_requestAdapter_a539af006419f2e9: function(arg0, arg1) {
      const ret = arg0.requestAdapter(arg1);
      return ret;
    },
    __wbg_requestDevice_5cb8a582e55d08cb: function(arg0, arg1) {
      const ret = arg0.requestDevice(arg1);
      return ret;
    },
    __wbg_resolve_0076e10020304ede: function(arg0) {
      const ret = Promise.resolve(arg0);
      return ret;
    },
    __wbg_setBindGroup_11bdbb60cc8b54b9: function() {
      return handleError(function(arg0, arg1, arg2, arg3, arg4, arg5, arg6) {
        arg0.setBindGroup(arg1 >>> 0, arg2, getArrayU32FromWasm0(arg3, arg4), arg5, arg6 >>> 0);
      }, arguments);
    },
    __wbg_setBindGroup_418c3e0eb6943ce0: function(arg0, arg1, arg2) {
      arg0.setBindGroup(arg1 >>> 0, arg2);
    },
    __wbg_setIndexBuffer_241097e303986c14: function(arg0, arg1, arg2, arg3, arg4) {
      arg0.setIndexBuffer(arg1, __wbindgen_enum_GpuIndexFormat[arg2], arg3, arg4);
    },
    __wbg_setPipeline_b6f981027e02cd16: function(arg0, arg1) {
      arg0.setPipeline(arg1);
    },
    __wbg_setScissorRect_889235eeb784732b: function(arg0, arg1, arg2, arg3, arg4) {
      arg0.setScissorRect(arg1 >>> 0, arg2 >>> 0, arg3 >>> 0, arg4 >>> 0);
    },
    __wbg_setVertexBuffer_6db3b60e99280744: function(arg0, arg1, arg2, arg3) {
      arg0.setVertexBuffer(arg1 >>> 0, arg2, arg3);
    },
    __wbg_setVertexBuffer_cbf4ca1627c02f4c: function(arg0, arg1, arg2, arg3, arg4) {
      arg0.setVertexBuffer(arg1 >>> 0, arg2, arg3, arg4);
    },
    __wbg_setViewport_c0be08dcc8965ccf: function(arg0, arg1, arg2, arg3, arg4, arg5, arg6) {
      arg0.setViewport(arg1, arg2, arg3, arg4, arg5, arg6);
    },
    __wbg_set_145a351398b48c65: function() {
      return handleError(function(arg0, arg1, arg2) {
        const ret = Reflect.set(arg0, arg1, arg2);
        return ret;
      }, arguments);
    },
    __wbg_set_34d08fd992d43d61: function(arg0, arg1, arg2) {
      arg0.set(arg1, arg2 >>> 0);
    },
    __wbg_set_a_82818effc94f6256: function(arg0, arg1) {
      arg0.a = arg1;
    },
    __wbg_set_access_a099cfbbeec9b96f: function(arg0, arg1) {
      arg0.access = __wbindgen_enum_GpuStorageTextureAccess[arg1];
    },
    __wbg_set_address_mode_u_a68737cf5d288f95: function(arg0, arg1) {
      arg0.addressModeU = __wbindgen_enum_GpuAddressMode[arg1];
    },
    __wbg_set_address_mode_v_b1c3c45933f540d1: function(arg0, arg1) {
      arg0.addressModeV = __wbindgen_enum_GpuAddressMode[arg1];
    },
    __wbg_set_address_mode_w_889c31cf7022c764: function(arg0, arg1) {
      arg0.addressModeW = __wbindgen_enum_GpuAddressMode[arg1];
    },
    __wbg_set_alpha_106f21a936a85eba: function(arg0, arg1) {
      arg0.alpha = arg1;
    },
    __wbg_set_alpha_mode_5544568dbac50280: function(arg0, arg1) {
      arg0.alphaMode = __wbindgen_enum_GpuCanvasAlphaMode[arg1];
    },
    __wbg_set_alpha_to_coverage_enabled_3372ce329447b8f1: function(arg0, arg1) {
      arg0.alphaToCoverageEnabled = arg1 !== 0;
    },
    __wbg_set_array_layer_count_22afa0a979e4ad55: function(arg0, arg1) {
      arg0.arrayLayerCount = arg1 >>> 0;
    },
    __wbg_set_array_stride_f64_6816040e5e7598c3: function(arg0, arg1) {
      arg0.arrayStride = arg1;
    },
    __wbg_set_aspect_a48d046965270281: function(arg0, arg1) {
      arg0.aspect = __wbindgen_enum_GpuTextureAspect[arg1];
    },
    __wbg_set_aspect_b1a9909bf315433f: function(arg0, arg1) {
      arg0.aspect = __wbindgen_enum_GpuTextureAspect[arg1];
    },
    __wbg_set_attributes_9e38cb1dde387a5b: function(arg0, arg1, arg2) {
      arg0.attributes = getArrayJsValueViewFromWasm0(arg1, arg2);
    },
    __wbg_set_b_a3297ee7e7cac3a8: function(arg0, arg1) {
      arg0.b = arg1;
    },
    __wbg_set_base_array_layer_2435ba92c80346ae: function(arg0, arg1) {
      arg0.baseArrayLayer = arg1 >>> 0;
    },
    __wbg_set_base_mip_level_8b6093e875e7c65d: function(arg0, arg1) {
      arg0.baseMipLevel = arg1 >>> 0;
    },
    __wbg_set_beginning_of_pass_write_index_e552c5e8b8bbf52f: function(arg0, arg1) {
      arg0.beginningOfPassWriteIndex = arg1 >>> 0;
    },
    __wbg_set_bind_group_layouts_458c44ba55100b82: function(arg0, arg1, arg2) {
      arg0.bindGroupLayouts = getArrayJsValueViewFromWasm0(arg1, arg2);
    },
    __wbg_set_binding_81b3fac7f7acaf8d: function(arg0, arg1) {
      arg0.binding = arg1 >>> 0;
    },
    __wbg_set_binding_b6cee57f35ac5190: function(arg0, arg1) {
      arg0.binding = arg1 >>> 0;
    },
    __wbg_set_blend_1a801617945f7945: function(arg0, arg1) {
      arg0.blend = arg1;
    },
    __wbg_set_buffer_1548ae88a9188037: function(arg0, arg1) {
      arg0.buffer = arg1;
    },
    __wbg_set_buffer_8d0ac64ad20dfc84: function(arg0, arg1) {
      arg0.buffer = arg1;
    },
    __wbg_set_buffers_5d0e0c50791f710e: function(arg0, arg1, arg2) {
      arg0.buffers = getArrayJsValueViewFromWasm0(arg1, arg2);
    },
    __wbg_set_bytes_per_row_c28583f0063160f1: function(arg0, arg1) {
      arg0.bytesPerRow = arg1 >>> 0;
    },
    __wbg_set_clear_value_gpu_color_dict_a9f763e8372ac1de: function(arg0, arg1) {
      arg0.clearValue = arg1;
    },
    __wbg_set_code_5d5b0b9e2fd0dca7: function(arg0, arg1, arg2) {
      arg0.code = getStringFromWasm0(arg1, arg2);
    },
    __wbg_set_color_8ecace4011f47d2e: function(arg0, arg1) {
      arg0.color = arg1;
    },
    __wbg_set_color_attachments_622fe2d5997fda7a: function(arg0, arg1, arg2) {
      arg0.colorAttachments = getArrayJsValueViewFromWasm0(arg1, arg2);
    },
    __wbg_set_compare_080c9e492ff36990: function(arg0, arg1) {
      arg0.compare = __wbindgen_enum_GpuCompareFunction[arg1];
    },
    __wbg_set_compare_817cf3695599eaa6: function(arg0, arg1) {
      arg0.compare = __wbindgen_enum_GpuCompareFunction[arg1];
    },
    __wbg_set_count_8ff0c9474e39a849: function(arg0, arg1) {
      arg0.count = arg1 >>> 0;
    },
    __wbg_set_cull_mode_85d2b4ab0ce3a564: function(arg0, arg1) {
      arg0.cullMode = __wbindgen_enum_GpuCullMode[arg1];
    },
    __wbg_set_depth_bias_95abf479cae3f3cd: function(arg0, arg1) {
      arg0.depthBias = arg1;
    },
    __wbg_set_depth_bias_clamp_ba3d0b8348151350: function(arg0, arg1) {
      arg0.depthBiasClamp = arg1;
    },
    __wbg_set_depth_bias_slope_scale_6b2584d93f5b9cd2: function(arg0, arg1) {
      arg0.depthBiasSlopeScale = arg1;
    },
    __wbg_set_depth_clear_value_e30a4c754c6b3b26: function(arg0, arg1) {
      arg0.depthClearValue = arg1;
    },
    __wbg_set_depth_compare_a90de4e3714397ab: function(arg0, arg1) {
      arg0.depthCompare = __wbindgen_enum_GpuCompareFunction[arg1];
    },
    __wbg_set_depth_fail_op_b5c64541d1b6b482: function(arg0, arg1) {
      arg0.depthFailOp = __wbindgen_enum_GpuStencilOperation[arg1];
    },
    __wbg_set_depth_load_op_932888016d762d3e: function(arg0, arg1) {
      arg0.depthLoadOp = __wbindgen_enum_GpuLoadOp[arg1];
    },
    __wbg_set_depth_or_array_layers_e2f074a0284e4806: function(arg0, arg1) {
      arg0.depthOrArrayLayers = arg1 >>> 0;
    },
    __wbg_set_depth_read_only_be790175a1c2db9a: function(arg0, arg1) {
      arg0.depthReadOnly = arg1 !== 0;
    },
    __wbg_set_depth_stencil_attachment_54a8922f5fbe08bf: function(arg0, arg1) {
      arg0.depthStencilAttachment = arg1;
    },
    __wbg_set_depth_stencil_b7cffc59ad4da529: function(arg0, arg1) {
      arg0.depthStencil = arg1;
    },
    __wbg_set_depth_store_op_9054814f164ab55d: function(arg0, arg1) {
      arg0.depthStoreOp = __wbindgen_enum_GpuStoreOp[arg1];
    },
    __wbg_set_depth_write_enabled_31a821ee1fb3b0b3: function(arg0, arg1) {
      arg0.depthWriteEnabled = arg1 !== 0;
    },
    __wbg_set_device_210484a77b675c9c: function(arg0, arg1) {
      arg0.device = arg1;
    },
    __wbg_set_dimension_3da9d03131a9f446: function(arg0, arg1) {
      arg0.dimension = __wbindgen_enum_GpuTextureDimension[arg1];
    },
    __wbg_set_dimension_56332450afa3e0c0: function(arg0, arg1) {
      arg0.dimension = __wbindgen_enum_GpuTextureViewDimension[arg1];
    },
    __wbg_set_dst_factor_865ba9aaf187890c: function(arg0, arg1) {
      arg0.dstFactor = __wbindgen_enum_GpuBlendFactor[arg1];
    },
    __wbg_set_end_of_pass_write_index_8f164f9e60d4ad16: function(arg0, arg1) {
      arg0.endOfPassWriteIndex = arg1 >>> 0;
    },
    __wbg_set_entries_6f866302103b81e9: function(arg0, arg1, arg2) {
      arg0.entries = getArrayJsValueViewFromWasm0(arg1, arg2);
    },
    __wbg_set_entries_f26b77ab9548e906: function(arg0, arg1, arg2) {
      arg0.entries = getArrayJsValueViewFromWasm0(arg1, arg2);
    },
    __wbg_set_entry_point_71cef95c137b5774: function(arg0, arg1, arg2) {
      arg0.entryPoint = getStringFromWasm0(arg1, arg2);
    },
    __wbg_set_entry_point_b70f98f5025a114d: function(arg0, arg1, arg2) {
      arg0.entryPoint = getStringFromWasm0(arg1, arg2);
    },
    __wbg_set_external_texture_7f966c604c4f8098: function(arg0, arg1) {
      arg0.externalTexture = arg1;
    },
    __wbg_set_fail_op_d59d0187e4111dfe: function(arg0, arg1) {
      arg0.failOp = __wbindgen_enum_GpuStencilOperation[arg1];
    },
    __wbg_set_format_23f7f32549751d43: function(arg0, arg1) {
      arg0.format = __wbindgen_enum_GpuTextureFormat[arg1];
    },
    __wbg_set_format_283dca56552f07a3: function(arg0, arg1) {
      arg0.format = __wbindgen_enum_GpuTextureFormat[arg1];
    },
    __wbg_set_format_5080a858117ad2c1: function(arg0, arg1) {
      arg0.format = __wbindgen_enum_GpuVertexFormat[arg1];
    },
    __wbg_set_format_66735b94bd868ba2: function(arg0, arg1) {
      arg0.format = __wbindgen_enum_GpuTextureFormat[arg1];
    },
    __wbg_set_format_7f2bdbfb101b1ae1: function(arg0, arg1) {
      arg0.format = __wbindgen_enum_GpuTextureFormat[arg1];
    },
    __wbg_set_format_92732ea75d3b79f5: function(arg0, arg1) {
      arg0.format = __wbindgen_enum_GpuTextureFormat[arg1];
    },
    __wbg_set_format_f009e603f7d4c28e: function(arg0, arg1) {
      arg0.format = __wbindgen_enum_GpuTextureFormat[arg1];
    },
    __wbg_set_fragment_d2b0ec97d7cf8d47: function(arg0, arg1) {
      arg0.fragment = arg1;
    },
    __wbg_set_front_face_d3f8a2e07e7b25dd: function(arg0, arg1) {
      arg0.frontFace = __wbindgen_enum_GpuFrontFace[arg1];
    },
    __wbg_set_g_b527ee8a9bed553d: function(arg0, arg1) {
      arg0.g = arg1;
    },
    __wbg_set_has_dynamic_offset_0c72ffa900c5a269: function(arg0, arg1) {
      arg0.hasDynamicOffset = arg1 !== 0;
    },
    __wbg_set_height_c9789c1c77eaedff: function(arg0, arg1) {
      arg0.height = arg1 >>> 0;
    },
    __wbg_set_height_f6619158e5735877: function(arg0, arg1) {
      arg0.height = arg1 >>> 0;
    },
    __wbg_set_height_fde391767df1ce27: function(arg0, arg1) {
      arg0.height = arg1 >>> 0;
    },
    __wbg_set_label_17202740051e9722: function(arg0, arg1, arg2) {
      arg0.label = getStringFromWasm0(arg1, arg2);
    },
    __wbg_set_label_2fefb39c0e0dbbe8: function(arg0, arg1, arg2) {
      arg0.label = getStringFromWasm0(arg1, arg2);
    },
    __wbg_set_label_3cb2322e6f6db14c: function(arg0, arg1, arg2) {
      arg0.label = getStringFromWasm0(arg1, arg2);
    },
    __wbg_set_label_3f2ccaafef5ff7c9: function(arg0, arg1, arg2) {
      arg0.label = getStringFromWasm0(arg1, arg2);
    },
    __wbg_set_label_612add98a4398f92: function(arg0, arg1, arg2) {
      arg0.label = getStringFromWasm0(arg1, arg2);
    },
    __wbg_set_label_70a09ee68d6b1b26: function(arg0, arg1, arg2) {
      arg0.label = getStringFromWasm0(arg1, arg2);
    },
    __wbg_set_label_92cd3811e96b487c: function(arg0, arg1, arg2) {
      arg0.label = getStringFromWasm0(arg1, arg2);
    },
    __wbg_set_label_9c2a186152427ee0: function(arg0, arg1, arg2) {
      arg0.label = getStringFromWasm0(arg1, arg2);
    },
    __wbg_set_label_c3eaf136aa464cba: function(arg0, arg1, arg2) {
      arg0.label = getStringFromWasm0(arg1, arg2);
    },
    __wbg_set_label_c7987704d29f284b: function(arg0, arg1, arg2) {
      arg0.label = getStringFromWasm0(arg1, arg2);
    },
    __wbg_set_label_cfe64bca8945ee30: function(arg0, arg1, arg2) {
      arg0.label = getStringFromWasm0(arg1, arg2);
    },
    __wbg_set_label_e02179cf97e95763: function(arg0, arg1, arg2) {
      arg0.label = getStringFromWasm0(arg1, arg2);
    },
    __wbg_set_label_ee172cd5f6a96961: function(arg0, arg1, arg2) {
      arg0.label = getStringFromWasm0(arg1, arg2);
    },
    __wbg_set_layout_454e3a091b390cd4: function(arg0, arg1) {
      arg0.layout = arg1;
    },
    __wbg_set_layout_75dc1ca3f2421cff: function(arg0, arg1) {
      arg0.layout = arg1;
    },
    __wbg_set_layout_gpu_auto_layout_mode_06a2b95af1043098: function(arg0, arg1) {
      arg0.layout = __wbindgen_enum_GpuAutoLayoutMode[arg1];
    },
    __wbg_set_load_op_c56b1269acc2d51f: function(arg0, arg1) {
      arg0.loadOp = __wbindgen_enum_GpuLoadOp[arg1];
    },
    __wbg_set_lod_max_clamp_db24179f67f3aa31: function(arg0, arg1) {
      arg0.lodMaxClamp = arg1;
    },
    __wbg_set_lod_min_clamp_2bbce566e9fefa04: function(arg0, arg1) {
      arg0.lodMinClamp = arg1;
    },
    __wbg_set_mag_filter_db8e6b42d4f8846d: function(arg0, arg1) {
      arg0.magFilter = __wbindgen_enum_GpuFilterMode[arg1];
    },
    __wbg_set_mapped_at_creation_3f320fef6761b02c: function(arg0, arg1) {
      arg0.mappedAtCreation = arg1 !== 0;
    },
    __wbg_set_mask_c1079e551ec360dc: function(arg0, arg1) {
      arg0.mask = arg1 >>> 0;
    },
    __wbg_set_max_anisotropy_84749fdcec362dc4: function(arg0, arg1) {
      arg0.maxAnisotropy = arg1;
    },
    __wbg_set_min_binding_size_f64_897e3cd4496ddec9: function(arg0, arg1) {
      arg0.minBindingSize = arg1;
    },
    __wbg_set_min_filter_d435bbfc5a637757: function(arg0, arg1) {
      arg0.minFilter = __wbindgen_enum_GpuFilterMode[arg1];
    },
    __wbg_set_mip_level_count_047936c630acee7b: function(arg0, arg1) {
      arg0.mipLevelCount = arg1 >>> 0;
    },
    __wbg_set_mip_level_count_44bc46a1ae6f6daa: function(arg0, arg1) {
      arg0.mipLevelCount = arg1 >>> 0;
    },
    __wbg_set_mip_level_f3745730372683d5: function(arg0, arg1) {
      arg0.mipLevel = arg1 >>> 0;
    },
    __wbg_set_mipmap_filter_62fb49a84b0747ff: function(arg0, arg1) {
      arg0.mipmapFilter = __wbindgen_enum_GpuMipmapFilterMode[arg1];
    },
    __wbg_set_mode_7edfbc344ef9c650: function(arg0, arg1) {
      arg0.mode = __wbindgen_enum_GpuCanvasToneMappingMode[arg1];
    },
    __wbg_set_module_392eeaa269f203b0: function(arg0, arg1) {
      arg0.module = arg1;
    },
    __wbg_set_module_715d37652c4998ec: function(arg0, arg1) {
      arg0.module = arg1;
    },
    __wbg_set_multisample_ff72a7a5456cbeb7: function(arg0, arg1) {
      arg0.multisample = arg1;
    },
    __wbg_set_multisampled_039f032dc4b67367: function(arg0, arg1) {
      arg0.multisampled = arg1 !== 0;
    },
    __wbg_set_offset_f64_127e8a0aa5c5485a: function(arg0, arg1) {
      arg0.offset = arg1;
    },
    __wbg_set_offset_f64_457756429ede426d: function(arg0, arg1) {
      arg0.offset = arg1;
    },
    __wbg_set_offset_f64_a903425d5a8e5815: function(arg0, arg1) {
      arg0.offset = arg1;
    },
    __wbg_set_onuncapturederror_bb813f1102da2296: function(arg0, arg1) {
      arg0.onuncapturederror = arg1;
    },
    __wbg_set_operation_00a77386523b88f9: function(arg0, arg1) {
      arg0.operation = __wbindgen_enum_GpuBlendOperation[arg1];
    },
    __wbg_set_origin_gpu_origin_3d_dict_0619d4860adb4eb6: function(arg0, arg1) {
      arg0.origin = arg1;
    },
    __wbg_set_pass_op_3cf10feb3d76ab97: function(arg0, arg1) {
      arg0.passOp = __wbindgen_enum_GpuStencilOperation[arg1];
    },
    __wbg_set_power_preference_b42d00a8facfbade: function(arg0, arg1) {
      arg0.powerPreference = __wbindgen_enum_GpuPowerPreference[arg1];
    },
    __wbg_set_primitive_e796cf76f0ff89f3: function(arg0, arg1) {
      arg0.primitive = arg1;
    },
    __wbg_set_query_set_f030702f1b69199f: function(arg0, arg1) {
      arg0.querySet = arg1;
    },
    __wbg_set_r_6ece4d74af63364f: function(arg0, arg1) {
      arg0.r = arg1;
    },
    __wbg_set_required_features_bbab71414c45e621: function(arg0, arg1, arg2) {
      arg0.requiredFeatures = getArrayJsValueViewFromWasm0(arg1, arg2);
    },
    __wbg_set_required_limits_837f62d865e7cfac: function(arg0, arg1) {
      arg0.requiredLimits = arg1;
    },
    __wbg_set_resolve_target_gpu_texture_view_e4c1e3bbb8c27d87: function(arg0, arg1) {
      arg0.resolveTarget = arg1;
    },
    __wbg_set_resource_8fd8658b30d86ecf: function(arg0, arg1) {
      arg0.resource = arg1;
    },
    __wbg_set_resource_gpu_buffer_binding_33099b25da65b610: function(arg0, arg1) {
      arg0.resource = arg1;
    },
    __wbg_set_resource_gpu_texture_view_4cffe7bc7c8e5cbe: function(arg0, arg1) {
      arg0.resource = arg1;
    },
    __wbg_set_rows_per_image_c6d50d227e634379: function(arg0, arg1) {
      arg0.rowsPerImage = arg1 >>> 0;
    },
    __wbg_set_sample_count_481c255a12054e1d: function(arg0, arg1) {
      arg0.sampleCount = arg1 >>> 0;
    },
    __wbg_set_sample_type_ebc5fcd029513bda: function(arg0, arg1) {
      arg0.sampleType = __wbindgen_enum_GpuTextureSampleType[arg1];
    },
    __wbg_set_sampler_89cb4a7efcfc6005: function(arg0, arg1) {
      arg0.sampler = arg1;
    },
    __wbg_set_shader_location_3fb9f6a012eba494: function(arg0, arg1) {
      arg0.shaderLocation = arg1 >>> 0;
    },
    __wbg_set_size_f64_2f591b0654540477: function(arg0, arg1) {
      arg0.size = arg1;
    },
    __wbg_set_size_f64_e844c985b8f95261: function(arg0, arg1) {
      arg0.size = arg1;
    },
    __wbg_set_size_gpu_extent_3d_dict_adf57388ab1d4f18: function(arg0, arg1) {
      arg0.size = arg1;
    },
    __wbg_set_src_factor_6f2c9ec8e4d3d979: function(arg0, arg1) {
      arg0.srcFactor = __wbindgen_enum_GpuBlendFactor[arg1];
    },
    __wbg_set_stencil_back_c54d0443b8b6a957: function(arg0, arg1) {
      arg0.stencilBack = arg1;
    },
    __wbg_set_stencil_clear_value_a321b0e045bfd8c2: function(arg0, arg1) {
      arg0.stencilClearValue = arg1 >>> 0;
    },
    __wbg_set_stencil_front_3ff3f8385852efff: function(arg0, arg1) {
      arg0.stencilFront = arg1;
    },
    __wbg_set_stencil_load_op_37d20deccb26a0f1: function(arg0, arg1) {
      arg0.stencilLoadOp = __wbindgen_enum_GpuLoadOp[arg1];
    },
    __wbg_set_stencil_read_mask_021ef4271b24352c: function(arg0, arg1) {
      arg0.stencilReadMask = arg1 >>> 0;
    },
    __wbg_set_stencil_read_only_75fe66a2356d6e92: function(arg0, arg1) {
      arg0.stencilReadOnly = arg1 !== 0;
    },
    __wbg_set_stencil_store_op_501f91638dd386e6: function(arg0, arg1) {
      arg0.stencilStoreOp = __wbindgen_enum_GpuStoreOp[arg1];
    },
    __wbg_set_stencil_write_mask_ec1c12237e094bdd: function(arg0, arg1) {
      arg0.stencilWriteMask = arg1 >>> 0;
    },
    __wbg_set_step_mode_3cbbdeba1e5dfd62: function(arg0, arg1) {
      arg0.stepMode = __wbindgen_enum_GpuVertexStepMode[arg1];
    },
    __wbg_set_storage_texture_786aea7c5773b6c1: function(arg0, arg1) {
      arg0.storageTexture = arg1;
    },
    __wbg_set_store_op_678f33376d741711: function(arg0, arg1) {
      arg0.storeOp = __wbindgen_enum_GpuStoreOp[arg1];
    },
    __wbg_set_strip_index_format_70313df755145d5e: function(arg0, arg1) {
      arg0.stripIndexFormat = __wbindgen_enum_GpuIndexFormat[arg1];
    },
    __wbg_set_targets_674b33931e512fb1: function(arg0, arg1, arg2) {
      arg0.targets = getArrayJsValueViewFromWasm0(arg1, arg2);
    },
    __wbg_set_texture_95f2bfdf7767e76f: function(arg0, arg1) {
      arg0.texture = arg1;
    },
    __wbg_set_texture_a33be3fe02ac6264: function(arg0, arg1) {
      arg0.texture = arg1;
    },
    __wbg_set_timestamp_writes_de6a09f299b71b76: function(arg0, arg1) {
      arg0.timestampWrites = arg1;
    },
    __wbg_set_tone_mapping_320c1aad31db2e7f: function(arg0, arg1) {
      arg0.toneMapping = arg1;
    },
    __wbg_set_topology_b92cfe523bd9653b: function(arg0, arg1) {
      arg0.topology = __wbindgen_enum_GpuPrimitiveTopology[arg1];
    },
    __wbg_set_type_43e0092f16775979: function(arg0, arg1) {
      arg0.type = __wbindgen_enum_GpuSamplerBindingType[arg1];
    },
    __wbg_set_type_79cec55caf4cdb6d: function(arg0, arg1) {
      arg0.type = __wbindgen_enum_GpuBufferBindingType[arg1];
    },
    __wbg_set_unclipped_depth_32b7caf29fa5633d: function(arg0, arg1) {
      arg0.unclippedDepth = arg1 !== 0;
    },
    __wbg_set_usage_1ee33d98267e787d: function(arg0, arg1) {
      arg0.usage = arg1 >>> 0;
    },
    __wbg_set_usage_2365e2704b1fdb10: function(arg0, arg1) {
      arg0.usage = arg1 >>> 0;
    },
    __wbg_set_usage_d53ee6f0c7aedbfa: function(arg0, arg1) {
      arg0.usage = arg1 >>> 0;
    },
    __wbg_set_usage_f3e34822998d2147: function(arg0, arg1) {
      arg0.usage = arg1 >>> 0;
    },
    __wbg_set_vertex_77ed7a1229239b5a: function(arg0, arg1) {
      arg0.vertex = arg1;
    },
    __wbg_set_view_dimension_893e2d16561e56e8: function(arg0, arg1) {
      arg0.viewDimension = __wbindgen_enum_GpuTextureViewDimension[arg1];
    },
    __wbg_set_view_dimension_f2c5fe4bf927c3fe: function(arg0, arg1) {
      arg0.viewDimension = __wbindgen_enum_GpuTextureViewDimension[arg1];
    },
    __wbg_set_view_formats_427069064d8b7139: function(arg0, arg1, arg2) {
      arg0.viewFormats = getArrayJsValueViewFromWasm0(arg1, arg2);
    },
    __wbg_set_view_formats_9c2f01a6f3b365c7: function(arg0, arg1, arg2) {
      arg0.viewFormats = getArrayJsValueViewFromWasm0(arg1, arg2);
    },
    __wbg_set_view_gpu_texture_view_35f4655788535c4d: function(arg0, arg1) {
      arg0.view = arg1;
    },
    __wbg_set_view_gpu_texture_view_a532c825c52042c0: function(arg0, arg1) {
      arg0.view = arg1;
    },
    __wbg_set_visibility_d8a6821789538c25: function(arg0, arg1) {
      arg0.visibility = arg1 >>> 0;
    },
    __wbg_set_width_0ca908d167690992: function(arg0, arg1) {
      arg0.width = arg1 >>> 0;
    },
    __wbg_set_width_b0e1267db4b196b5: function(arg0, arg1) {
      arg0.width = arg1 >>> 0;
    },
    __wbg_set_width_b20525f5f4df4eb8: function(arg0, arg1) {
      arg0.width = arg1 >>> 0;
    },
    __wbg_set_write_mask_42d89f182ade6b2d: function(arg0, arg1) {
      arg0.writeMask = arg1 >>> 0;
    },
    __wbg_set_x_f470b03dd54724cd: function(arg0, arg1) {
      arg0.x = arg1 >>> 0;
    },
    __wbg_set_y_4c44eb40ebca5bfc: function(arg0, arg1) {
      arg0.y = arg1 >>> 0;
    },
    __wbg_set_z_2e6820ef0f5821ed: function(arg0, arg1) {
      arg0.z = arg1 >>> 0;
    },
    __wbg_static_accessor_GLOBAL_266715b9d96ba635: function() {
      const ret = typeof global === "undefined" ? null : global;
      return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
    },
    __wbg_static_accessor_GLOBAL_THIS_10fb7dc1ae063179: function() {
      const ret = typeof globalThis === "undefined" ? null : globalThis;
      return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
    },
    __wbg_static_accessor_SELF_0b583911f537483a: function() {
      const ret = typeof self === "undefined" ? null : self;
      return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
    },
    __wbg_static_accessor_WINDOW_d7f903d1508cbdc4: function() {
      const ret = typeof window === "undefined" ? null : window;
      return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
    },
    __wbg_subgroupMaxSize_b43be0aa16182403: function(arg0) {
      const ret = arg0.subgroupMaxSize;
      return ret;
    },
    __wbg_subgroupMinSize_03feb6ee0cda6775: function(arg0) {
      const ret = arg0.subgroupMinSize;
      return ret;
    },
    __wbg_submit_077c85cc28e36892: function(arg0, arg1, arg2) {
      arg0.submit(getArrayJsValueViewFromWasm0(arg1, arg2));
    },
    __wbg_then_c8a35d4ad59c6b8e: function(arg0, arg1) {
      const ret = arg0.then(arg1);
      return ret;
    },
    __wbg_then_c949d5a25a4e78f8: function(arg0, arg1, arg2) {
      const ret = arg0.then(arg1, arg2);
      return ret;
    },
    __wbg_then_e71170d78fcf8954: function(arg0, arg1) {
      const ret = arg0.then(arg1);
      return ret;
    },
    __wbg_unconfigure_835307f58dc68d80: function(arg0) {
      arg0.unconfigure();
    },
    __wbg_unmap_6a96b14c9ef5f7f5: function(arg0) {
      arg0.unmap();
    },
    __wbg_valueOf_02fd4d101b7553c7: function(arg0) {
      const ret = arg0.valueOf();
      return ret;
    },
    __wbg_warn_13abc63e0d4b3527: function(arg0) {
      console.warn(arg0);
    },
    __wbg_width_3d0dce3d9892e35e: function(arg0) {
      const ret = arg0.width;
      return ret;
    },
    __wbg_width_7a1b335e4a553dc3: function(arg0) {
      const ret = arg0.width;
      return ret;
    },
    __wbg_writeBuffer_f4bb3f54adfe1330: function() {
      return handleError(function(arg0, arg1, arg2, arg3, arg4, arg5, arg6) {
        arg0.writeBuffer(arg1, arg2, getArrayU8FromWasm0(arg3, arg4), arg5, arg6);
      }, arguments);
    },
    __wbg_writeTexture_30e592e8c061c3d9: function() {
      return handleError(function(arg0, arg1, arg2, arg3, arg4, arg5) {
        arg0.writeTexture(arg1, getArrayU8FromWasm0(arg2, arg3), arg4, arg5);
      }, arguments);
    },
    __wbindgen_generic_0000000000000001: function(arg0, arg1) {
      const ret = makeMutClosure(arg0, arg1, wasm_bindgen_265b8cf50d59c234___convert__closures_____invoke___wasm_bindgen_265b8cf50d59c234___JsValue__core_608f92abc48d28da___result__Result_____wasm_bindgen_265b8cf50d59c234___JsError___true_);
      return ret;
    },
    __wbindgen_generic_0000000000000002: function(arg0, arg1) {
      const ret = makeMutClosure(arg0, arg1, wasm_bindgen_265b8cf50d59c234___convert__closures_____invoke___wasm_bindgen_265b8cf50d59c234___sys__JsNullable_wgpu_eaa100529841c49f___backend__webgpu__webgpu_sys__gen_GpuError__GpuError___core_608f92abc48d28da___result__Result_____wasm_bindgen_265b8cf50d59c234___JsError___true_);
      return ret;
    },
    __wbindgen_generic_0000000000000003: function(arg0, arg1) {
      const ret = makeMutClosure(arg0, arg1, wasm_bindgen_265b8cf50d59c234___convert__closures_____invoke___wgpu_eaa100529841c49f___backend__webgpu__webgpu_sys__gen_GpuDeviceLostInfo__GpuDeviceLostInfo______true_);
      return ret;
    },
    __wbindgen_generic_0000000000000004: function(arg0, arg1) {
      const ret = makeMutClosure(arg0, arg1, wasm_bindgen_265b8cf50d59c234___convert__closures_____invoke___wgpu_eaa100529841c49f___backend__webgpu__webgpu_sys__gen_GpuDeviceLostInfo__GpuDeviceLostInfo______true__119);
      return ret;
    },
    __wbindgen_generic_0000000000000005: function(arg0, arg1) {
      const ret = makeMutClosure(arg0, arg1, wasm_bindgen_265b8cf50d59c234___convert__closures_____invoke___wasm_bindgen_265b8cf50d59c234___sys__JsNullable_wgpu_eaa100529841c49f___backend__webgpu__webgpu_sys__gen_GpuError__GpuError___core_608f92abc48d28da___result__Result_____wasm_bindgen_265b8cf50d59c234___JsError___true__120);
      return ret;
    },
    __wbindgen_generic_0000000000000006: function(arg0, arg1) {
      const ret = makeMutClosure(arg0, arg1, wasm_bindgen_265b8cf50d59c234___convert__closures_____invoke___wasm_bindgen_265b8cf50d59c234___sys__JsNullable_wgpu_eaa100529841c49f___backend__webgpu__webgpu_sys__gen_GpuError__GpuError___core_608f92abc48d28da___result__Result_____wasm_bindgen_265b8cf50d59c234___JsError___true__121);
      return ret;
    },
    __wbindgen_generic_0000000000000007: function(arg0) {
      const ret = arg0;
      return ret;
    },
    __wbindgen_generic_0000000000000008: function(arg0, arg1) {
      const ret = getArrayU8FromWasm0(arg0, arg1);
      return ret;
    },
    __wbindgen_generic_0000000000000009: function(arg0, arg1) {
      const ret = getStringFromWasm0(arg0, arg1);
      return ret;
    },
    __wbindgen_init_externref_table: function() {
      const table = wasm.__wbindgen_externrefs;
      const offset = table.grow(4);
      table.set(0, void 0);
      table.set(offset + 0, void 0);
      table.set(offset + 1, null);
      table.set(offset + 2, true);
      table.set(offset + 3, false);
    }
  };
  return {
    __proto__: null,
    "./mb_host_bg.js": import0
  };
}
function wasm_bindgen_265b8cf50d59c234___convert__closures_____invoke___wgpu_eaa100529841c49f___backend__webgpu__webgpu_sys__gen_GpuDeviceLostInfo__GpuDeviceLostInfo______true_(arg0, arg1, arg2) {
  wasm.wasm_bindgen_265b8cf50d59c234___convert__closures_____invoke___wgpu_eaa100529841c49f___backend__webgpu__webgpu_sys__gen_GpuDeviceLostInfo__GpuDeviceLostInfo______true_(arg0, arg1, arg2);
}
function wasm_bindgen_265b8cf50d59c234___convert__closures_____invoke___wgpu_eaa100529841c49f___backend__webgpu__webgpu_sys__gen_GpuDeviceLostInfo__GpuDeviceLostInfo______true__119(arg0, arg1, arg2) {
  wasm.wasm_bindgen_265b8cf50d59c234___convert__closures_____invoke___wgpu_eaa100529841c49f___backend__webgpu__webgpu_sys__gen_GpuDeviceLostInfo__GpuDeviceLostInfo______true__119(arg0, arg1, arg2);
}
function wasm_bindgen_265b8cf50d59c234___convert__closures_____invoke___wasm_bindgen_265b8cf50d59c234___JsValue__core_608f92abc48d28da___result__Result_____wasm_bindgen_265b8cf50d59c234___JsError___true_(arg0, arg1, arg2) {
  const ret = wasm.wasm_bindgen_265b8cf50d59c234___convert__closures_____invoke___wasm_bindgen_265b8cf50d59c234___JsValue__core_608f92abc48d28da___result__Result_____wasm_bindgen_265b8cf50d59c234___JsError___true_(arg0, arg1, arg2);
  if (ret[1]) {
    throw takeFromExternrefTable0(ret[0]);
  }
}
function wasm_bindgen_265b8cf50d59c234___convert__closures_____invoke___wasm_bindgen_265b8cf50d59c234___sys__JsNullable_wgpu_eaa100529841c49f___backend__webgpu__webgpu_sys__gen_GpuError__GpuError___core_608f92abc48d28da___result__Result_____wasm_bindgen_265b8cf50d59c234___JsError___true_(arg0, arg1, arg2) {
  const ret = wasm.wasm_bindgen_265b8cf50d59c234___convert__closures_____invoke___wasm_bindgen_265b8cf50d59c234___sys__JsNullable_wgpu_eaa100529841c49f___backend__webgpu__webgpu_sys__gen_GpuError__GpuError___core_608f92abc48d28da___result__Result_____wasm_bindgen_265b8cf50d59c234___JsError___true_(arg0, arg1, arg2);
  if (ret[1]) {
    throw takeFromExternrefTable0(ret[0]);
  }
}
function wasm_bindgen_265b8cf50d59c234___convert__closures_____invoke___wasm_bindgen_265b8cf50d59c234___sys__JsNullable_wgpu_eaa100529841c49f___backend__webgpu__webgpu_sys__gen_GpuError__GpuError___core_608f92abc48d28da___result__Result_____wasm_bindgen_265b8cf50d59c234___JsError___true__120(arg0, arg1, arg2) {
  const ret = wasm.wasm_bindgen_265b8cf50d59c234___convert__closures_____invoke___wasm_bindgen_265b8cf50d59c234___sys__JsNullable_wgpu_eaa100529841c49f___backend__webgpu__webgpu_sys__gen_GpuError__GpuError___core_608f92abc48d28da___result__Result_____wasm_bindgen_265b8cf50d59c234___JsError___true__120(arg0, arg1, arg2);
  if (ret[1]) {
    throw takeFromExternrefTable0(ret[0]);
  }
}
function wasm_bindgen_265b8cf50d59c234___convert__closures_____invoke___wasm_bindgen_265b8cf50d59c234___sys__JsNullable_wgpu_eaa100529841c49f___backend__webgpu__webgpu_sys__gen_GpuError__GpuError___core_608f92abc48d28da___result__Result_____wasm_bindgen_265b8cf50d59c234___JsError___true__121(arg0, arg1, arg2) {
  const ret = wasm.wasm_bindgen_265b8cf50d59c234___convert__closures_____invoke___wasm_bindgen_265b8cf50d59c234___sys__JsNullable_wgpu_eaa100529841c49f___backend__webgpu__webgpu_sys__gen_GpuError__GpuError___core_608f92abc48d28da___result__Result_____wasm_bindgen_265b8cf50d59c234___JsError___true__121(arg0, arg1, arg2);
  if (ret[1]) {
    throw takeFromExternrefTable0(ret[0]);
  }
}
function wasm_bindgen_265b8cf50d59c234___convert__closures_____invoke___js_sys_1be3539c672eb81b___Function_fn_wasm_bindgen_265b8cf50d59c234___JsValue_____wasm_bindgen_265b8cf50d59c234___sys__Undefined___js_sys_1be3539c672eb81b___Function_fn_wasm_bindgen_265b8cf50d59c234___JsValue_____wasm_bindgen_265b8cf50d59c234___sys__Undefined_______true_(arg0, arg1, arg2, arg3) {
  wasm.wasm_bindgen_265b8cf50d59c234___convert__closures_____invoke___js_sys_1be3539c672eb81b___Function_fn_wasm_bindgen_265b8cf50d59c234___JsValue_____wasm_bindgen_265b8cf50d59c234___sys__Undefined___js_sys_1be3539c672eb81b___Function_fn_wasm_bindgen_265b8cf50d59c234___JsValue_____wasm_bindgen_265b8cf50d59c234___sys__Undefined_______true_(arg0, arg1, arg2, arg3);
}
var __wbindgen_enum_GpuAddressMode = ["clamp-to-edge", "repeat", "mirror-repeat"];
var __wbindgen_enum_GpuAutoLayoutMode = ["auto"];
var __wbindgen_enum_GpuBlendFactor = ["zero", "one", "src", "one-minus-src", "src-alpha", "one-minus-src-alpha", "dst", "one-minus-dst", "dst-alpha", "one-minus-dst-alpha", "src-alpha-saturated", "constant", "one-minus-constant", "src1", "one-minus-src1", "src1-alpha", "one-minus-src1-alpha"];
var __wbindgen_enum_GpuBlendOperation = ["add", "subtract", "reverse-subtract", "min", "max"];
var __wbindgen_enum_GpuBufferBindingType = ["uniform", "storage", "read-only-storage"];
var __wbindgen_enum_GpuCanvasAlphaMode = ["opaque", "premultiplied"];
var __wbindgen_enum_GpuCanvasToneMappingMode = ["standard", "extended"];
var __wbindgen_enum_GpuCompareFunction = ["never", "less", "equal", "less-equal", "greater", "not-equal", "greater-equal", "always"];
var __wbindgen_enum_GpuCullMode = ["none", "front", "back"];
var __wbindgen_enum_GpuDeviceLostReason = ["unknown", "destroyed"];
var __wbindgen_enum_GpuFilterMode = ["nearest", "linear"];
var __wbindgen_enum_GpuFrontFace = ["ccw", "cw"];
var __wbindgen_enum_GpuIndexFormat = ["uint16", "uint32"];
var __wbindgen_enum_GpuLoadOp = ["load", "clear"];
var __wbindgen_enum_GpuMipmapFilterMode = ["nearest", "linear"];
var __wbindgen_enum_GpuPowerPreference = ["low-power", "high-performance"];
var __wbindgen_enum_GpuPrimitiveTopology = ["point-list", "line-list", "line-strip", "triangle-list", "triangle-strip"];
var __wbindgen_enum_GpuSamplerBindingType = ["filtering", "non-filtering", "comparison"];
var __wbindgen_enum_GpuStencilOperation = ["keep", "zero", "replace", "invert", "increment-clamp", "decrement-clamp", "increment-wrap", "decrement-wrap"];
var __wbindgen_enum_GpuStorageTextureAccess = ["write-only", "read-only", "read-write"];
var __wbindgen_enum_GpuStoreOp = ["store", "discard"];
var __wbindgen_enum_GpuTextureAspect = ["all", "stencil-only", "depth-only"];
var __wbindgen_enum_GpuTextureDimension = ["1d", "2d", "3d"];
var __wbindgen_enum_GpuTextureFormat = ["r8unorm", "r8snorm", "r8uint", "r8sint", "r16unorm", "r16snorm", "r16uint", "r16sint", "r16float", "rg8unorm", "rg8snorm", "rg8uint", "rg8sint", "r32uint", "r32sint", "r32float", "rg16unorm", "rg16snorm", "rg16uint", "rg16sint", "rg16float", "rgba8unorm", "rgba8unorm-srgb", "rgba8snorm", "rgba8uint", "rgba8sint", "bgra8unorm", "bgra8unorm-srgb", "rgb9e5ufloat", "rgb10a2uint", "rgb10a2unorm", "rg11b10ufloat", "rg32uint", "rg32sint", "rg32float", "rgba16unorm", "rgba16snorm", "rgba16uint", "rgba16sint", "rgba16float", "rgba32uint", "rgba32sint", "rgba32float", "stencil8", "depth16unorm", "depth24plus", "depth24plus-stencil8", "depth32float", "depth32float-stencil8", "bc1-rgba-unorm", "bc1-rgba-unorm-srgb", "bc2-rgba-unorm", "bc2-rgba-unorm-srgb", "bc3-rgba-unorm", "bc3-rgba-unorm-srgb", "bc4-r-unorm", "bc4-r-snorm", "bc5-rg-unorm", "bc5-rg-snorm", "bc6h-rgb-ufloat", "bc6h-rgb-float", "bc7-rgba-unorm", "bc7-rgba-unorm-srgb", "etc2-rgb8unorm", "etc2-rgb8unorm-srgb", "etc2-rgb8a1unorm", "etc2-rgb8a1unorm-srgb", "etc2-rgba8unorm", "etc2-rgba8unorm-srgb", "eac-r11unorm", "eac-r11snorm", "eac-rg11unorm", "eac-rg11snorm", "astc-4x4-unorm", "astc-4x4-unorm-srgb", "astc-5x4-unorm", "astc-5x4-unorm-srgb", "astc-5x5-unorm", "astc-5x5-unorm-srgb", "astc-6x5-unorm", "astc-6x5-unorm-srgb", "astc-6x6-unorm", "astc-6x6-unorm-srgb", "astc-8x5-unorm", "astc-8x5-unorm-srgb", "astc-8x6-unorm", "astc-8x6-unorm-srgb", "astc-8x8-unorm", "astc-8x8-unorm-srgb", "astc-10x5-unorm", "astc-10x5-unorm-srgb", "astc-10x6-unorm", "astc-10x6-unorm-srgb", "astc-10x8-unorm", "astc-10x8-unorm-srgb", "astc-10x10-unorm", "astc-10x10-unorm-srgb", "astc-12x10-unorm", "astc-12x10-unorm-srgb", "astc-12x12-unorm", "astc-12x12-unorm-srgb"];
var __wbindgen_enum_GpuTextureSampleType = ["float", "unfilterable-float", "depth", "sint", "uint"];
var __wbindgen_enum_GpuTextureViewDimension = ["1d", "2d", "2d-array", "cube", "cube-array", "3d"];
var __wbindgen_enum_GpuVertexFormat = ["uint8", "uint8x2", "uint8x4", "sint8", "sint8x2", "sint8x4", "unorm8", "unorm8x2", "unorm8x4", "snorm8", "snorm8x2", "snorm8x4", "uint16", "uint16x2", "uint16x4", "sint16", "sint16x2", "sint16x4", "unorm16", "unorm16x2", "unorm16x4", "snorm16", "snorm16x2", "snorm16x4", "float16", "float16x2", "float16x4", "float32", "float32x2", "float32x3", "float32x4", "uint32", "uint32x2", "uint32x3", "uint32x4", "sint32", "sint32x2", "sint32x3", "sint32x4", "unorm10-10-10-2", "unorm8x4-bgra"];
var __wbindgen_enum_GpuVertexStepMode = ["vertex", "instance"];
function addToExternrefTable0(obj) {
  const idx = wasm.__externref_table_alloc();
  wasm.__wbindgen_externrefs.set(idx, obj);
  return idx;
}
var CLOSURE_DTORS = typeof FinalizationRegistry === "undefined" ? { register: () => {
}, unregister: () => {
} } : new FinalizationRegistry((state) => wasm.__wbindgen_destroy_closure(state.a, state.b));
function debugString(val) {
  const type = typeof val;
  if (type == "number" || type == "boolean" || val == null) {
    return `${val}`;
  }
  if (type == "string") {
    return `"${val}"`;
  }
  if (type == "symbol") {
    const description = val.description;
    if (description == null) {
      return "Symbol";
    } else {
      return `Symbol(${description})`;
    }
  }
  if (type == "function") {
    const name = val.name;
    if (typeof name == "string" && name.length > 0) {
      return `Function(${name})`;
    } else {
      return "Function";
    }
  }
  if (Array.isArray(val)) {
    const length = val.length;
    let debug = "[";
    if (length > 0) {
      debug += debugString(val[0]);
    }
    for (let i = 1; i < length; i++) {
      debug += ", " + debugString(val[i]);
    }
    debug += "]";
    return debug;
  }
  const builtInMatches = /\[object ([^\]]+)\]/.exec(toString.call(val));
  let className;
  if (builtInMatches && builtInMatches.length > 1) {
    className = builtInMatches[1];
  } else {
    return toString.call(val);
  }
  if (className == "Object") {
    try {
      return "Object(" + JSON.stringify(val) + ")";
    } catch (_) {
      return "Object";
    }
  }
  if (val instanceof Error) {
    return `${val.name}: ${val.message}
${val.stack}`;
  }
  return className;
}
function getArrayF32FromWasm0(ptr, len) {
  ptr = ptr >>> 0;
  return getFloat32ArrayMemory0().subarray(ptr / 4, ptr / 4 + len);
}
function getArrayJsValueFromWasm0(ptr, len) {
  ptr = ptr >>> 0;
  const mem = getDataViewMemory0();
  const result = [];
  for (let i = ptr; i < ptr + 4 * len; i += 4) {
    result.push(wasm.__wbindgen_externrefs.get(mem.getUint32(i, true)));
  }
  wasm.__externref_drop_slice(ptr, len);
  return result;
}
function getArrayJsValueViewFromWasm0(ptr, len) {
  ptr = ptr >>> 0;
  const mem = getDataViewMemory0();
  const result = [];
  for (let i = ptr; i < ptr + 4 * len; i += 4) {
    result.push(wasm.__wbindgen_externrefs.get(mem.getUint32(i, true)));
  }
  return result;
}
function getArrayU32FromWasm0(ptr, len) {
  ptr = ptr >>> 0;
  return getUint32ArrayMemory0().subarray(ptr / 4, ptr / 4 + len);
}
function getArrayU8FromWasm0(ptr, len) {
  ptr = ptr >>> 0;
  return getUint8ArrayMemory0().subarray(ptr / 1, ptr / 1 + len);
}
var cachedDataViewMemory0 = null;
function getDataViewMemory0() {
  if (cachedDataViewMemory0 === null || cachedDataViewMemory0.buffer.detached === true || cachedDataViewMemory0.buffer.detached === void 0 && cachedDataViewMemory0.buffer !== wasm.memory.buffer) {
    cachedDataViewMemory0 = new DataView(wasm.memory.buffer);
  }
  return cachedDataViewMemory0;
}
var cachedFloat32ArrayMemory0 = null;
function getFloat32ArrayMemory0() {
  if (cachedFloat32ArrayMemory0 === null || cachedFloat32ArrayMemory0.byteLength === 0) {
    cachedFloat32ArrayMemory0 = new Float32Array(wasm.memory.buffer);
  }
  return cachedFloat32ArrayMemory0;
}
function getStringFromWasm0(ptr, len) {
  return decodeText(ptr >>> 0, len);
}
var cachedUint32ArrayMemory0 = null;
function getUint32ArrayMemory0() {
  if (cachedUint32ArrayMemory0 === null || cachedUint32ArrayMemory0.byteLength === 0) {
    cachedUint32ArrayMemory0 = new Uint32Array(wasm.memory.buffer);
  }
  return cachedUint32ArrayMemory0;
}
var cachedUint8ArrayMemory0 = null;
function getUint8ArrayMemory0() {
  if (cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0) {
    cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer);
  }
  return cachedUint8ArrayMemory0;
}
function handleError(f, args) {
  try {
    return f.apply(this, args);
  } catch (e) {
    const idx = addToExternrefTable0(e);
    wasm.__wbindgen_exn_store(idx);
  }
}
function isLikeNone(x) {
  return x === void 0 || x === null;
}
function makeMutClosure(arg0, arg1, f) {
  const state = { a: arg0, b: arg1, cnt: 1 };
  const real = (...args) => {
    state.cnt++;
    const a = state.a;
    state.a = 0;
    try {
      return f(a, state.b, ...args);
    } finally {
      state.a = a;
      real._wbg_cb_unref();
    }
  };
  real._wbg_cb_unref = () => {
    if (--state.cnt === 0) {
      wasm.__wbindgen_destroy_closure(state.a, state.b);
      state.a = 0;
      CLOSURE_DTORS.unregister(state);
    }
  };
  CLOSURE_DTORS.register(real, state, state);
  return real;
}
function passStringToWasm0(arg, malloc, realloc) {
  if (realloc === void 0) {
    const buf = cachedTextEncoder.encode(arg);
    const ptr2 = malloc(buf.length, 1) >>> 0;
    getUint8ArrayMemory0().subarray(ptr2, ptr2 + buf.length).set(buf);
    WASM_VECTOR_LEN = buf.length;
    return ptr2;
  }
  let len = arg.length;
  let ptr = malloc(len, 1) >>> 0;
  const mem = getUint8ArrayMemory0();
  let offset = 0;
  for (; offset < len; offset++) {
    const code = arg.charCodeAt(offset);
    if (code > 127) break;
    mem[ptr + offset] = code;
  }
  if (offset !== len) {
    if (offset !== 0) {
      arg = arg.slice(offset);
    }
    ptr = realloc(ptr, len, len = offset + arg.length * 3, 1) >>> 0;
    const view = getUint8ArrayMemory0().subarray(ptr + offset, ptr + len);
    const ret = cachedTextEncoder.encodeInto(arg, view);
    offset += ret.written;
    ptr = realloc(ptr, len, offset, 1) >>> 0;
  }
  WASM_VECTOR_LEN = offset;
  return ptr;
}
function takeFromExternrefTable0(idx) {
  const value = wasm.__wbindgen_externrefs.get(idx);
  wasm.__externref_table_dealloc(idx);
  return value;
}
var cachedTextDecoder = new TextDecoder("utf-8", { ignoreBOM: true, fatal: true });
cachedTextDecoder.decode();
var MAX_SAFARI_DECODE_BYTES = 2146435072;
var numBytesDecoded = 0;
function decodeText(ptr, len) {
  numBytesDecoded += len;
  if (numBytesDecoded >= MAX_SAFARI_DECODE_BYTES) {
    cachedTextDecoder = new TextDecoder("utf-8", { ignoreBOM: true, fatal: true });
    cachedTextDecoder.decode();
    numBytesDecoded = len;
  }
  return cachedTextDecoder.decode(getUint8ArrayMemory0().subarray(ptr, ptr + len));
}
var cachedTextEncoder = new TextEncoder();
if (!("encodeInto" in cachedTextEncoder)) {
  cachedTextEncoder.encodeInto = function(arg, view) {
    const buf = cachedTextEncoder.encode(arg);
    view.set(buf);
    return {
      read: arg.length,
      written: buf.length
    };
  };
}
var WASM_VECTOR_LEN = 0;
var wasmModule;
var wasmInstance;
var wasm;
function __wbg_finalize_init(instance, module) {
  wasmInstance = instance;
  wasm = instance.exports;
  wasmModule = module;
  cachedDataViewMemory0 = null;
  cachedFloat32ArrayMemory0 = null;
  cachedUint32ArrayMemory0 = null;
  cachedUint8ArrayMemory0 = null;
  wasm.__wbindgen_start();
  return wasm;
}
async function __wbg_load(module, imports) {
  if (typeof Response === "function" && module instanceof Response) {
    if (!module.ok) {
      throw new Error(`failed to fetch Wasm: ${module.status} ${module.statusText} fetching '${module.url}'`);
    }
    if (typeof WebAssembly.instantiateStreaming === "function") {
      try {
        return await WebAssembly.instantiateStreaming(module, imports);
      } catch (e) {
        const validResponse = expectedResponseType(module.type);
        if (validResponse && module.headers.get("Content-Type") !== "application/wasm") {
          console.warn("`WebAssembly.instantiateStreaming` failed because your server does not serve Wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n", e);
        } else {
          throw e;
        }
      }
    }
    const bytes = await module.arrayBuffer();
    return await WebAssembly.instantiate(bytes, imports);
  } else {
    const instance = await WebAssembly.instantiate(module, imports);
    if (instance instanceof WebAssembly.Instance) {
      return { instance, module };
    } else {
      return instance;
    }
  }
  function expectedResponseType(type) {
    switch (type) {
      case "basic":
      case "cors":
      case "default":
        return true;
    }
    return false;
  }
}
async function __wbg_init(module_or_path) {
  if (wasm !== void 0) return wasm;
  if (module_or_path !== void 0) {
    if (Object.getPrototypeOf(module_or_path) === Object.prototype) {
      ({ module_or_path } = module_or_path);
    } else {
      console.warn("using deprecated parameters for the initialization function; pass a single object instead");
    }
  }
  if (module_or_path === void 0) {
    module_or_path = new URL("mb_host_bg.wasm", import.meta.url);
  }
  const imports = __wbg_get_imports();
  if (typeof module_or_path === "string" || typeof Request === "function" && module_or_path instanceof Request || typeof URL === "function" && module_or_path instanceof URL) {
    module_or_path = fetch(module_or_path);
  }
  const { instance, module } = await __wbg_load(await module_or_path, imports);
  return __wbg_finalize_init(instance, module);
}

// src/bridge.ts
var page = typeof window === "undefined" ? void 0 : window;
var native = page?.webkit?.messageHandlers?.mb;
var taps = [];
var sink = null;
function post(m) {
  if (sink) return sink(m);
  if (native) native.postMessage(m);
  else page?.dispatchEvent(new CustomEvent("mb:message", { detail: m }));
  for (const t of taps) t(m);
  if (m.op !== "heartbeat") console.log("[mb]", m.op, JSON.stringify(m));
}
var hasNative = native !== void 0;

// src/gpu.ts
var made = 0;
var reported = 0;
function countPipelines() {
  const proto = globalThis.GPUDevice?.prototype;
  if (!proto) return;
  for (const name of ["createRenderPipeline", "createComputePipeline", "createRenderPipelineAsync", "createComputePipelineAsync"]) {
    const f = proto[name];
    if (typeof f !== "function") continue;
    proto[name] = function(...args) {
      made++;
      return f.apply(this, args);
    };
  }
}
countPipelines();
var RETRY_MS = [50, 300, 1e3];
var MAX_REPLACEMENTS = 5;
var WINDOW_MS = 3e4;
var restoring = false;
var gaveUp = false;
var replaced = [];
function message(e) {
  return e instanceof Error ? `${e.name}: ${e.message}` : String(e);
}
function log(level, msg) {
  post({ op: "log", level, msg: `runtime: ${msg}` });
}
function flushLog() {
  for (const line of host_gpu_log()) log(2, line);
}
function present(draw) {
  let ok;
  try {
    ok = draw();
  } catch (e) {
    host_gpu_fault(message(e));
    ok = false;
  }
  flushLog();
  if (made > reported) {
    post({ op: "compiling", pipelines: made - reported });
    reported = made;
  }
  if (!ok) void restore();
}
async function restore() {
  if (restoring || gaveUp) return;
  restoring = true;
  const now = performance.now();
  replaced = replaced.filter((t) => now - t < WINDOW_MS);
  let why = `device replaced ${replaced.length} times in ${WINDOW_MS / 1e3} s`;
  if (replaced.length < MAX_REPLACEMENTS) {
    for (const wait of RETRY_MS) {
      await new Promise((r) => setTimeout(r, wait));
      try {
        await host_gpu_connect();
      } catch (e) {
        why = message(e);
        log(2, `gpu: no new device yet (${why})`);
        continue;
      }
      let ok = false;
      try {
        ok = host_gpu_install();
      } catch (e) {
        host_gpu_fault(message(e));
        why = message(e);
      }
      flushLog();
      if (ok) {
        replaced.push(performance.now());
        restoring = false;
        log(1, "gpu: new device ready");
        present(host_redraw);
        return;
      }
    }
  }
  restoring = false;
  gaveUp = true;
  log(2, `gpu: giving up on this page (${why})`);
  post({ op: "gpu-lost", message: why });
}
function debugGpu(kind) {
  if (kind === "lose") {
    host_gpu_debug_lose();
    return;
  }
  const g = globalThis;
  const proto = g.GPUCommandEncoder.prototype;
  const finish = proto.finish;
  proto.finish = function() {
    proto.finish = finish;
    throw new DOMException("GPUCommandEncoder.finish: Unable to finish.", "InvalidStateError");
  };
  if (kind === "gone") {
    const gpu = navigator.gpu;
    gpu.requestAdapter = () => Promise.resolve(null);
  }
}

// src/buses.ts
var BUS_MASTER = 0;
var BUS_MUSIC = 1;
var BUS_SFX = 2;
var BUS_COUNT = 3;
var FX_FIELDS = 8;
var MIN_RAMP = 0.01;
var OPEN_HZ = 2e4;
var LOW_HZ = 20;
var IR_MIN_INTERVAL = 200;
var IR_MIN_FADE = 0.05;
var MAX_DELAY = 2;
var defaults = () => ({
  lowpass: { cutoff: OPEN_HZ, q: Math.SQRT1_2 },
  highpass: { cutoff: LOW_HZ, q: Math.SQRT1_2 },
  reverb: { wet: 0, decay: 1.5, damping: 0.5, predelay: 0.02 },
  delay: { wet: 0, time: 0.3, feedback: 0.35, damping: 0.3 },
  volume: 1
});
var Buses = class {
  params = Array.from({ length: BUS_COUNT }, defaults);
  ctx = null;
  nodes = [];
  /** Builds every bus on a (new) context, master feeding `out`, with the current settings. */
  attach(ctx, out) {
    this.detach();
    this.ctx = ctx;
    for (let b = 0; b < BUS_COUNT; b++) {
      const input = ctx.createGain();
      const highpass = ctx.createBiquadFilter();
      highpass.type = "highpass";
      const lowpass = ctx.createBiquadFilter();
      lowpass.type = "lowpass";
      const volume = ctx.createGain();
      input.connect(highpass).connect(lowpass).connect(volume);
      this.nodes.push({ input, highpass, lowpass, volume, reverb: null, delay: null });
    }
    this.nodes[BUS_MUSIC].volume.connect(this.nodes[BUS_MASTER].input);
    this.nodes[BUS_SFX].volume.connect(this.nodes[BUS_MASTER].input);
    this.nodes[BUS_MASTER].volume.connect(out);
    for (let b = 0; b < BUS_COUNT; b++) this.apply(b, 0, true);
  }
  /** The context is gone (closed or replaced): forget its nodes, keep the settings. */
  detach() {
    for (const n of this.nodes) {
      if (n.reverb?.rebuild) clearTimeout(n.reverb.rebuild);
      if (n.reverb?.idle) clearTimeout(n.reverb.idle);
    }
    this.ctx = null;
    this.nodes = [];
  }
  /** Where a voice on `bus` connects, or null without a context. */
  input(bus) {
    return this.nodes[bus]?.input ?? this.nodes[BUS_SFX]?.input ?? null;
  }
  /** A new session: every bus back to its defaults, at once. */
  reset() {
    this.params = Array.from({ length: BUS_COUNT }, defaults);
    if (this.ctx) for (let b = 0; b < BUS_COUNT; b++) this.apply(b, 0, true);
  }
  /** mb_bus_fx: `f` is the struct's 8 fields as f32s, the first `ramp`.
   *  Returns 0, or −2 for an unknown bus or kind. `instant` skips the glide
   *  (fast-forwarding a restored session). */
  set(bus, kind, f, instant) {
    if (!Number.isInteger(bus) || bus < 0 || bus >= BUS_COUNT) return -2;
    const num = (i, lo, hi, dflt) => {
      const x = f[i] ?? NaN;
      return Number.isFinite(x) ? Math.min(Math.max(x, lo), hi) : dflt;
    };
    const ramp = num(0, 0, 60, 0);
    const p = this.params[bus];
    switch (kind) {
      case 1 /* Lowpass */:
      case 2 /* Highpass */: {
        const filter = kind === 1 /* Lowpass */ ? p.lowpass : p.highpass;
        filter.cutoff = num(1, LOW_HZ, OPEN_HZ, kind === 1 /* Lowpass */ ? OPEN_HZ : LOW_HZ);
        filter.q = num(2, 0.1, 30, Math.SQRT1_2);
        break;
      }
      case 3 /* Reverb */:
        p.reverb = { wet: num(1, 0, 1, 0), decay: num(2, 0.1, 10, 1.5), damping: num(3, 0, 1, 0.5), predelay: num(4, 0, 0.25, 0.02) };
        break;
      case 4 /* Delay */:
        p.delay = { wet: num(1, 0, 1, 0), time: num(2, 0.01, MAX_DELAY, 0.3), feedback: num(3, 0, 0.9, 0.35), damping: num(4, 0, 1, 0.3) };
        break;
      case 5 /* Volume */:
        p.volume = num(1, 0, 4, 1);
        break;
      default:
        return -2;
    }
    if (this.ctx) this.apply(bus, ramp, instant);
    return 0;
  }
  /** Moves the bus's nodes to its parameters. */
  apply(b, ramp, instant) {
    const ctx = this.ctx;
    const n = this.nodes[b];
    const p = this.params[b];
    const at = instant ? null : ctx.currentTime;
    const nyquist = ctx.sampleRate / 2;
    sweep(n.lowpass.frequency, p.lowpass.cutoff >= OPEN_HZ ? nyquist * 0.98 : Math.min(p.lowpass.cutoff, nyquist * 0.98), ramp, at);
    sweep(n.highpass.frequency, p.highpass.cutoff <= LOW_HZ ? 10 : p.highpass.cutoff, ramp, at);
    glide(n.lowpass.Q, qDb(p.lowpass.cutoff >= OPEN_HZ ? Math.SQRT1_2 : p.lowpass.q), ramp, at);
    glide(n.highpass.Q, qDb(p.highpass.cutoff <= LOW_HZ ? Math.SQRT1_2 : p.highpass.q), ramp, at);
    glide(n.volume.gain, p.volume, ramp, at);
    this.applyReverb(b, ramp, at);
    this.applyDelay(b, ramp, at);
  }
  applyReverb(b, ramp, at) {
    const ctx = this.ctx;
    const n = this.nodes[b];
    const p = this.params[b].reverb;
    if (!n.reverb) {
      if (p.wet <= 0) return;
      const wet = ctx.createGain();
      wet.gain.value = 0;
      const predelay = ctx.createDelay(0.5);
      predelay.delayTime.value = p.predelay;
      const conv = ctx.createConvolver();
      conv.normalize = false;
      conv.buffer = impulse(ctx, p.decay, p.damping);
      const level = ctx.createGain();
      n.lowpass.connect(wet).connect(predelay).connect(conv).connect(level).connect(n.volume);
      n.reverb = { wet, predelay, conv, level, decay: p.decay, damping: p.damping, builtAt: performance.now(), rebuild: null, idle: null };
      if (at === null) at = ctx.currentTime;
    }
    const r = n.reverb;
    glide(r.wet.gain, p.wet, ramp, at);
    glide(r.predelay.delayTime, p.predelay, ramp, at);
    if (r.decay !== p.decay || r.damping !== p.damping) this.scheduleIr(b, r, ramp);
    if (r.idle) clearTimeout(r.idle);
    r.idle = null;
    if (p.wet <= 0) {
      const ctxNow = ctx;
      r.idle = setTimeout(
        () => {
          r.idle = null;
          if (this.ctx !== ctxNow || this.nodes[b]?.reverb !== r || this.params[b].reverb.wet > 0) return;
          if (r.rebuild) clearTimeout(r.rebuild);
          r.wet.disconnect();
          r.level.disconnect();
          r.conv.disconnect();
          n.reverb = null;
        },
        (ramp + p.predelay + r.decay + 0.5) * 1e3
      );
    }
  }
  /** A new impulse response when the last was made long enough ago, with the latest values. */
  scheduleIr(b, r, ramp) {
    if (r.rebuild) return;
    const ctx = this.ctx;
    const wait = Math.max(0, r.builtAt + IR_MIN_INTERVAL - performance.now());
    r.rebuild = setTimeout(() => {
      r.rebuild = null;
      if (this.ctx !== ctx || this.nodes[b]?.reverb !== r) return;
      const p = this.params[b].reverb;
      if (r.decay === p.decay && r.damping === p.damping) return;
      const conv = ctx.createConvolver();
      conv.normalize = false;
      conv.buffer = impulse(ctx, p.decay, p.damping);
      const level = ctx.createGain();
      level.gain.value = 0;
      r.predelay.connect(conv).connect(level).connect(this.nodes[b].volume);
      const fade = Math.max(ramp, IR_MIN_FADE);
      const t = ctx.currentTime;
      level.gain.setTargetAtTime(1, t, fade / 4);
      r.level.gain.setTargetAtTime(0, t, fade / 4);
      const [oldConv, oldLevel] = [r.conv, r.level];
      setTimeout(() => {
        oldConv.disconnect();
        oldLevel.disconnect();
      }, (fade * 1.6 + 0.1) * 1e3);
      Object.assign(r, { conv, level, decay: p.decay, damping: p.damping, builtAt: performance.now() });
    }, wait);
  }
  applyDelay(b, ramp, at) {
    const ctx = this.ctx;
    const n = this.nodes[b];
    const p = this.params[b].delay;
    if (!n.delay) {
      if (p.wet <= 0) return;
      const wet = ctx.createGain();
      wet.gain.value = 0;
      const line = ctx.createDelay(MAX_DELAY);
      line.delayTime.value = p.time;
      const damp = ctx.createBiquadFilter();
      damp.type = "lowpass";
      const feedback = ctx.createGain();
      n.lowpass.connect(wet).connect(line).connect(damp).connect(n.volume);
      damp.connect(feedback).connect(line);
      n.delay = { wet, line, damp, feedback };
      if (at === null) at = ctx.currentTime;
    }
    const d = n.delay;
    glide(d.wet.gain, p.wet, ramp, at);
    glide(d.line.delayTime, p.time, ramp, at);
    glide(d.feedback.gain, p.feedback, ramp, at);
    sweep(d.damp.frequency, Math.min(2e4 * 0.05 ** p.damping, ctx.sampleRate * 0.49), ramp, at);
  }
  /** Preview aid (part of `mb.audio()`). */
  debug() {
    return this.params.map((p, b) => ({
      bus: ["master", "music", "sfx"][b],
      ...structuredClone(p),
      reverbBuilt: !!this.nodes[b]?.reverb,
      delayBuilt: !!this.nodes[b]?.delay
    }));
  }
};
function glide(param, v, ramp, at) {
  if (at === null) {
    param.cancelScheduledValues(0);
    param.value = v;
    return;
  }
  param.setTargetAtTime(v, at, Math.max(ramp, MIN_RAMP) / 4);
}
function sweep(param, hz, ramp, at) {
  if (at === null) {
    param.cancelScheduledValues(0);
    param.value = hz;
    return;
  }
  const from = param.value;
  param.cancelScheduledValues(at);
  param.setValueAtTime(from > 0 ? from : hz, at);
  param.exponentialRampToValueAtTime(hz, at + Math.max(ramp, MIN_RAMP));
}
function qDb(q2) {
  return 20 * Math.log10(q2);
}
function impulse(ctx, decay, damping) {
  const rate = ctx.sampleRate;
  const len = Math.max(1, Math.round(rate * (decay + 0.05)));
  const buf = ctx.createBuffer(2, len, rate);
  const top = 14e3;
  const bottom = top * 0.02 ** damping;
  let seed = 625341585;
  for (let ch = 0; ch < 2; ch++) {
    const d = buf.getChannelData(ch);
    let lp = 0;
    let energy = 0;
    for (let i = 0; i < len; i++) {
      const t = i / rate;
      seed ^= seed << 13;
      seed ^= seed >>> 17;
      seed ^= seed << 5;
      const noise = (seed >>> 0) / 2147483648 - 1;
      const fc = top * (bottom / top) ** Math.min(1, t / decay);
      lp += (noise - lp) * (1 - Math.exp(-2 * Math.PI * fc / rate));
      const v = lp * Math.exp(-6.9078 * t / decay) * Math.min(1, t / 0.01);
      d[i] = v;
      energy += v * v;
    }
    const k = 1 / Math.sqrt(energy || 1);
    for (let i = 0; i < len; i++) d[i] *= k;
  }
  return buf;
}

// src/audio.ts
var MAX_VOICES = 32;
var MASTER_GAIN = 0.8;
var GLIDE = 0.012;
var RESUME_GRACE_MS = 400;
var PRESENT_FRAMES = 1;
var FALLBACK_OUTPUT_LATENCY = 0.02;
var LATE_START_LEAD = 0.01;
var CLOCK_SMOOTHING = 0.05;
var CLOCK_SNAP = 0.02;
var CLOCK_HYSTERESIS = 1e-3;
var RESCHEDULE_AT = 1e-3;
var CLOCK_SETTLE_FRAMES = 120;
var MAX_TIMING_LOGS = 12;
var MIN_FADE = 3e-3;
var RELEASE_TC = 6.9;
var Audio = class {
  ctx = null;
  /** Mute and master level, after the buses and before the limiter. */
  master = null;
  buses = new Buses();
  decoder = null;
  sounds = [];
  /** Decoded audio by source bytes (shared across sessions via the asset cache). */
  decoded = /* @__PURE__ */ new WeakMap();
  voices = /* @__PURE__ */ new Map();
  nextVoice = 1;
  /** The page is active (shown and running), so it may hold a running context. */
  active = false;
  muted = false;
  /** The player turned game sound off (app setting); independent of prerolling. */
  userMuted = false;
  generation = 0;
  warned = false;
  /** Fast-forwarding a restored session (watch mode): nothing starts playing. */
  ff = false;
  silence = null;
  // --- the clock: game time → wall time → context time ----------------------
  /** The current frame: its rAF timestamp (ms; NaN = no frame since the clock
   *  was invalidated), its game time and game seconds per wall second. */
  frameNow = NaN;
  frameGame = 0;
  frameScale = 1;
  lastFrameNow = NaN;
  /** Smoothed display frame period (ms), and the presentation delay in use. */
  period = 1e3 / 60;
  presentMs = 1e3 / 60 * PRESENT_FRAMES;
  /** Context seconds − performance.now seconds for the sample being heard:
   *  smoothed, and the value in use (hysteresis). */
  offsetEst = null;
  offset = null;
  farReadings = 0;
  via = "latency";
  /** This context has given a sane getOutputTimestamp; stop falling back. */
  otsSeen = false;
  /** Frames since this context's first clock reading (for the settled log). */
  clockFrames = -1;
  /** The previous frame's mapping, to notice when it moves. */
  prevMap = null;
  timingLogs = 0;
  constructor() {
    const unlock = () => this.unlock();
    for (const ev of ["pointerdown", "pointerup", "touchend", "click", "keydown"]) {
      if (typeof window !== "undefined") window.addEventListener(ev, unlock, { capture: true, passive: true });
    }
  }
  /** WebKit plays Web Audio as "ambient" (muted by the silent switch)
   *  unless the page asks for playback (PLAN Q14). */
  setPlayback(on) {
    const session = navigator.audioSession;
    if (session) {
      session.type = on ? "playback" : "ambient";
      post({ op: "log", level: 0, msg: `runtime: audio session ${session.type}` });
    } else if (on && !this.silence) {
      this.silence = new window.Audio(new URL("silence.wav", import.meta.url).href);
      this.silence.loop = true;
      void this.silence.play().catch((e) => post({ op: "log", level: 2, msg: `runtime: silent audio failed: ${e}` }));
      post({ op: "log", level: 0, msg: "runtime: audio session via silent media element" });
    } else if (!on && this.silence) {
      this.silence.pause();
      this.silence = null;
    }
  }
  // --- lifecycle ---------------------------------------------------------------
  /** The page is shown and running: create (or revive) the context and start loops. */
  resume() {
    this.active = true;
    if (this.quiet) {
      this.quiet = false;
      this.applyGain();
    }
    if (!this.ctx) {
      this.createContext();
      return;
    }
    const ctx = this.ctx;
    void ctx.resume().catch(() => {
    });
    setTimeout(() => {
      if (this.active && this.ctx === ctx && ctx.state !== "running") this.rebuild(`resume left it ${ctx.state}`);
    }, RESUME_GRACE_MS);
  }
  /** The page is going off screen (or is a neighbour): stop holding audio.
   *  Every voice pauses; play_at voices are rescheduled from game time on resume.
   *  `quiet`: a pause on the current card (the player will likely be right
   *  back): the context keeps running, silenced, so resuming needn't restart
   *  it; on iOS restarting it reactivates the audio session, which can hold
   *  up the app (and the controller input it forwards) for seconds. */
  suspend(quiet = false) {
    this.active = false;
    this.quiet = quiet;
    this.applyGain();
    if (!quiet) void this.ctx?.suspend().catch(() => {
    });
    this.invalidateClock(true);
  }
  /** Paused quietly (`suspend(true)`): silent, the context still running. */
  quiet = false;
  /** Silences output without stopping voices (a feed neighbour prerolling). */
  setMuted(on) {
    this.muted = on;
    this.applyGain();
  }
  /** The app's sound setting: silent while off, whatever else is going on. */
  setUserMuted(on) {
    this.userMuted = on;
    this.applyGain();
  }
  applyGain() {
    if (this.master) this.master.gain.value = this.muted || this.userMuted || this.quiet ? 0 : MASTER_GAIN;
  }
  /** Watch mode: a restored session runs many frames in a moment. Meanwhile
   *  mb_play one-shots are dropped (they'd all be late) and nothing starts;
   *  afterwards loops start and play_at voices are scheduled from game time,
   *  so music picks up where the run had got to. */
  fastForward(on) {
    this.ff = on;
    this.invalidateClock(true);
    if (!on) this.flush();
  }
  /** A new game session: the old session's voices stop and sound handles
   *  restart from 1. Decoded audio is kept and reused. */
  newSession() {
    for (const id of [...this.voices.keys()]) this.stop(id);
    this.buses.reset();
    this.sounds = [];
    this.generation++;
    this.timingLogs = 0;
    this.frameNow = NaN;
    this.prevMap = null;
  }
  createContext() {
    const ctx = new AudioContext({ latencyHint: "interactive" });
    const master = ctx.createGain();
    master.gain.value = this.muted || this.userMuted ? 0 : MASTER_GAIN;
    const limiter = ctx.createDynamicsCompressor();
    limiter.threshold.value = -9;
    limiter.knee.value = 6;
    limiter.ratio.value = 12;
    limiter.attack.value = 3e-3;
    limiter.release.value = 0.15;
    master.connect(limiter).connect(ctx.destination);
    ctx.onstatechange = () => {
      post({ op: "log", level: 0, msg: `runtime: audio ${ctx.state}` });
      if (ctx.state !== "running" && this.ctx === ctx) this.invalidateClock(false);
      if (ctx.state === "interrupted" && this.active && this.ctx === ctx) {
        setTimeout(() => {
          if (this.active && this.ctx === ctx && ctx.state !== "running") {
            void ctx.resume().catch(() => {
            });
            setTimeout(() => this.active && this.ctx === ctx && ctx.state !== "running" && this.rebuild("interrupted"), RESUME_GRACE_MS);
          }
        }, 100);
      }
    };
    this.ctx = ctx;
    this.master = master;
    this.buses.attach(ctx, master);
    this.otsSeen = false;
    this.clockFrames = -1;
    this.invalidateClock(false);
    post({ op: "log", level: 0, msg: `runtime: audio context created (${ctx.state})` });
    void ctx.resume().catch(() => {
    });
    for (const [id, v] of this.voices) {
      if (v.at !== null || v.nodes) continue;
      if (v.loop) this.tryStart(id, v);
      else this.voices.delete(id);
    }
    this.flush();
  }
  /** Replaces a context that won't run with a fresh one. */
  rebuild(why) {
    post({ op: "log", level: 1, msg: `runtime: rebuilding audio (${why})` });
    const old = this.ctx;
    for (const v of this.voices.values()) v.nodes = null;
    this.ctx = null;
    this.master = null;
    this.buses.detach();
    void old?.close().catch(() => {
    });
    this.createContext();
  }
  unlock() {
    if (!this.active) return;
    if (!this.ctx) this.createContext();
    const ctx = this.ctx;
    if (ctx.state === "running") return;
    const src = ctx.createBufferSource();
    src.buffer = ctx.createBuffer(1, 1, ctx.sampleRate);
    src.connect(ctx.destination);
    src.start();
    ctx.resume().catch((e) => post({ op: "log", level: 2, msg: `runtime: audio resume failed: ${e}` }));
  }
  // --- the clock -----------------------------------------------------------------
  /** Called once per frame, after game time has advanced and before
   *  mb_update: `now` is the frame's requestAnimationFrame timestamp,
   *  `game` its game time, `scale` game seconds per wall second. */
  frame(now, game, scale) {
    if (this.ff) {
      this.frameGame = game;
      return;
    }
    if (Number.isFinite(this.lastFrameNow)) {
      const p = now - this.lastFrameNow;
      if (p >= 4 && p <= 50) this.period += (p - this.period) * 0.05;
    }
    this.lastFrameNow = now;
    const present2 = this.period * PRESENT_FRAMES;
    if (Math.abs(present2 - this.presentMs) > 1) this.presentMs = present2;
    this.frameNow = now;
    this.frameGame = game;
    this.frameScale = scale > 0 ? scale : 1;
    this.measure();
    if (this.offset === null || !this.ctx) {
      this.prevMap = null;
      return;
    }
    const map = { now, game, scale: this.frameScale, present: this.presentMs, offset: this.offset };
    const prev = this.prevMap;
    this.prevMap = map;
    if (prev) {
      const before = (prev.now + prev.present + (game - prev.game) * 1e3 / prev.scale) / 1e3 + prev.offset;
      const after = (now + map.present) / 1e3 + map.offset;
      if (Math.abs(after - before) > RESCHEDULE_AT) this.reschedule();
    }
    this.flush();
    for (const v of this.voices.values()) if (v.stopAt !== null && v.nodes && !Number.isFinite(v.stopWhen)) this.applyStop(v);
    if (this.clockFrames >= 0 && ++this.clockFrames === CLOCK_SETTLE_FRAMES) this.logClock("settled");
  }
  /** Context time at which game time `at` is heard, with the current frame's mapping. */
  ctxTimeOf(at) {
    return (this.frameNow + this.presentMs + (at - this.frameGame) * 1e3 / this.frameScale) / 1e3 + (this.offset ?? 0);
  }
  clockReady() {
    return Number.isFinite(this.frameNow) && this.offset !== null && this.ctx?.state === "running";
  }
  /** Output latency the context reports (s), with a fallback for what it doesn't. */
  reportedLatency() {
    const ctx = this.ctx;
    const out = ctx.outputLatency;
    return (ctx.baseLatency || 0) + (typeof out === "number" && out > 0 ? out : FALLBACK_OUTPUT_LATENCY);
  }
  /** Reads the context's clock against performance.now and updates the offset. */
  measure() {
    const ctx = this.ctx;
    if (!ctx || ctx.state !== "running") return;
    const nowOffset = ctx.currentTime - performance.now() / 1e3;
    let raw = null;
    let via = "latency";
    if (typeof ctx.getOutputTimestamp === "function") {
      const ts = ctx.getOutputTimestamp();
      const c = ts.contextTime ?? 0;
      const p = ts.performanceTime ?? 0;
      if (c > 0 && p > 0) {
        const o = c - p / 1e3;
        const behind = nowOffset - o;
        if (behind > -5e-3 && behind < 1) {
          raw = o;
          via = "getOutputTimestamp";
          this.otsSeen = true;
        }
      }
    }
    if (raw === null) {
      if (this.otsSeen) return;
      raw = nowOffset - this.reportedLatency();
    }
    const switched = via !== this.via;
    this.via = via;
    if (this.offsetEst === null || switched) {
      this.offsetEst = raw;
      this.farReadings = 0;
    } else if (Math.abs(raw - this.offsetEst) > CLOCK_SNAP) {
      if (++this.farReadings < 3) return;
      post({ op: "log", level: 1, msg: `runtime: audio clock moved ${((raw - this.offsetEst) * 1e3).toFixed(1)} ms (output route change?)` });
      this.offsetEst = raw;
      this.farReadings = 0;
      this.clockFrames = 0;
    } else {
      this.farReadings = 0;
      this.offsetEst += (raw - this.offsetEst) * CLOCK_SMOOTHING;
    }
    const first = this.offset === null;
    if (first || Math.abs(this.offsetEst - this.offset) > CLOCK_HYSTERESIS) this.offset = this.offsetEst;
    if (first && this.clockFrames < 0) {
      this.clockFrames = 0;
      this.logClock("first");
    }
  }
  /** Measurement aid (shows in the iOS app's console as `runtime: audio clock …`). */
  logClock(when) {
    const ctx = this.ctx;
    if (!ctx || this.offset === null) return;
    const ms = (s) => typeof s === "number" ? `${(s * 1e3).toFixed(1)} ms` : "n/a";
    const trails = ctx.currentTime - performance.now() / 1e3 - this.offset;
    post({
      op: "log",
      level: 1,
      msg: `runtime: audio clock (${when}): baseLatency ${ms(ctx.baseLatency)}, outputLatency ${ms(ctx.outputLatency)}, getOutputTimestamp ${typeof ctx.getOutputTimestamp === "function" ? "yes" : "no"}, via ${this.via}, heard ${ms(trails)} after currentTime, frame ${this.period.toFixed(1)} ms, present +${this.presentMs.toFixed(1)} ms, ctx\u2212perf ${this.offset.toFixed(4)} s, ${ctx.sampleRate} Hz`
    });
  }
  /** The clock can't be trusted (suspended, a new context, a stopped timeline):
   *  play_at voices stop and are rescheduled once it's known again. */
  invalidateClock(frameToo) {
    if (frameToo) {
      this.frameNow = NaN;
      this.lastFrameNow = NaN;
    }
    this.offsetEst = null;
    this.offset = null;
    this.farReadings = 0;
    this.prevMap = null;
    for (const v of this.voices.values()) if (v.at !== null) this.stopNodes(v);
  }
  /** The mapping moved: play_at voices that haven't started yet are rescheduled. */
  reschedule() {
    const ctx = this.ctx;
    if (!ctx) return;
    for (const v of this.voices.values()) {
      if (v.at !== null && v.nodes && v.start > ctx.currentTime + LATE_START_LEAD) this.stopNodes(v);
      else if (v.nodes && v.stopAt !== null && v.stopWhen > ctx.currentTime + LATE_START_LEAD) v.stopWhen = NaN;
    }
  }
  // --- sounds and voices -------------------------------------------------------
  /** Returns a sound handle (≥ 1) and decodes in the background. */
  load(bytes) {
    const handle = this.sounds.push(null);
    const gen = this.generation;
    let decoding = this.decoded.get(bytes);
    if (!decoding && typeof OfflineAudioContext === "undefined") return handle;
    if (!decoding) {
      this.decoder ??= new OfflineAudioContext({ numberOfChannels: 1, length: 1, sampleRate: 48e3 });
      decoding = this.decoder.decodeAudioData(bytes.slice().buffer);
      this.decoded.set(bytes, decoding);
      decoding.then(
        (buf) => post({ op: "log", level: 0, msg: `runtime: sound decoded (${buf.duration.toFixed(2)} s)` }),
        () => {
        }
      );
    }
    decoding.then((buf) => {
      if (gen !== this.generation) return;
      this.sounds[handle - 1] = buf;
      this.flush();
    }).catch((e) => post({ op: "log", level: 2, msg: `runtime: sound ${handle} failed to decode: ${e}` }));
    return handle;
  }
  /** mb_play (`at` undefined: as soon as it can), mb_play_at (heard at game time `at`) and
   *  mb_play_from (heard at `at` from `from` seconds into the sound, rising over `fadeIn` seconds). */
  play(sound, vol, pan, pitch, loop, at, from = 0, fadeIn = 0) {
    const id = this.nextVoice++;
    if (sound < 1 || sound > this.sounds.length) return id;
    if (at === void 0 && !loop && (!this.sounds[sound - 1] || !this.ctx || this.ff)) return id;
    if (this.voices.size >= MAX_VOICES) {
      let victim;
      for (const [k, other] of this.voices) {
        if (!other.loop) {
          victim = k;
          break;
        }
      }
      this.stop(victim ?? this.voices.keys().next().value);
    }
    const v = {
      sound,
      vol,
      pan,
      pitch,
      loop,
      at: at ?? null,
      when: NaN,
      start: NaN,
      offset: 0,
      from: Number.isFinite(from) && from > 0 ? from : 0,
      fadeIn: Number.isFinite(fadeIn) && fadeIn > 0 ? Math.min(fadeIn, 60) : 0,
      stopAt: null,
      stopFade: 0,
      stopWhen: NaN,
      bus: BUS_SFX,
      nodes: null
    };
    this.voices.set(id, v);
    this.tryStart(id, v);
    return id;
  }
  /** Starts every voice that's waiting and now can. */
  flush() {
    for (const [id, v] of this.voices) if (!v.nodes) this.tryStart(id, v);
  }
  /** Starts a voice if it can: decoded, a context, and for play_at a known clock. */
  tryStart(id, v) {
    const ctx = this.ctx;
    const buf = this.sounds[v.sound - 1];
    if (v.nodes || !ctx || !this.master || !buf || this.ff) return;
    if (v.at === null) {
      if (ctx.state !== "running" && !this.warned) {
        this.warned = true;
        post({ op: "log", level: 2, msg: `runtime: playing while audio is ${ctx.state}` });
      }
      this.startNodes(id, v, buf, 0, 0);
      return;
    }
    if (!this.clockReady()) return;
    const when = this.ctxTimeOf(v.at);
    const earliest = ctx.currentTime + LATE_START_LEAD;
    let start = when;
    const from = v.loop ? v.from % buf.duration : v.from;
    let offset = from;
    if (!v.loop && from >= buf.duration || v.stopAt !== null && this.ctxTimeOf(v.stopAt) + v.stopFade <= Math.max(when, earliest)) {
      this.voices.delete(id);
      return;
    }
    if (when < earliest) {
      const late = earliest - when;
      offset = from + late * pitchOf(v);
      if (v.loop) {
        offset %= buf.duration;
      } else if (offset - from > (buf.duration - from) / 2) {
        this.voices.delete(id);
        this.timingLog(`runtime: voice ${id} skipped, ${(late * 1e3).toFixed(1)} ms late (sound ${buf.duration.toFixed(2)} s)`);
        return;
      }
      start = earliest;
      if (late > 2e-3) this.timingLog(`runtime: voice ${id}${v.loop ? " (loop)" : ""} started ${(late * 1e3).toFixed(1)} ms late, offset into its sound`);
    }
    v.when = when;
    this.startNodes(id, v, buf, start, offset);
  }
  timingLog(msg) {
    if (this.timingLogs++ < MAX_TIMING_LOGS) post({ op: "log", level: 0, msg });
  }
  startNodes(id, v, buf, start, offset) {
    const ctx = this.ctx;
    const src = ctx.createBufferSource();
    src.buffer = buf;
    src.loop = v.loop;
    const pan = ctx.createStereoPanner();
    const gain = ctx.createGain();
    const fadeIn = ctx.createGain();
    const fadeOut = ctx.createGain();
    src.connect(pan).connect(gain).connect(fadeIn).connect(fadeOut).connect(this.buses.input(v.bus));
    v.nodes = { src, pan, gain, fadeIn, fadeOut };
    v.start = start;
    v.offset = offset;
    v.stopWhen = NaN;
    this.apply(v, true);
    const into = Number.isFinite(v.when) ? Math.max(0, start - v.when) : 0;
    if (v.fadeIn > 0 && into < v.fadeIn) {
      const p0 = into / v.fadeIn;
      const n = 64;
      const curve = new Float32Array(n);
      for (let i = 0; i < n; i++) curve[i] = Math.sin((p0 + (1 - p0) * i / (n - 1)) * Math.PI / 2);
      fadeIn.gain.setValueCurveAtTime(curve, start, Math.max(MIN_FADE, (1 - p0) * v.fadeIn));
    } else if (offset > 0) {
      fadeIn.gain.setValueAtTime(0, start);
      fadeIn.gain.linearRampToValueAtTime(1, start + MIN_FADE);
    }
    src.onended = () => {
      if (this.voices.get(id) === v && v.nodes?.src === src) this.voices.delete(id);
    };
    src.start(start, offset);
    if (v.stopAt !== null) this.applyStop(v);
  }
  /** Schedules a voice's release (mb_voice_stop_at) on its nodes, once the clock is known. */
  applyStop(v) {
    const ctx = this.ctx;
    const n = v.nodes;
    if (!ctx || !n || v.stopAt === null || !this.clockReady()) return;
    const now = ctx.currentTime;
    const at = this.ctxTimeOf(v.stopAt);
    const end = at + v.stopFade;
    if (end <= v.start) {
      v.stopWhen = v.start;
      n.src.stop(v.start);
      return;
    }
    const t = Math.max(at, now, v.start);
    v.stopWhen = t;
    const g = n.fadeOut.gain;
    g.cancelScheduledValues(0);
    if (end - t <= MIN_FADE) {
      g.setValueAtTime(1, t);
      g.linearRampToValueAtTime(0, t + MIN_FADE);
      n.src.stop(t + 2 * MIN_FADE);
      return;
    }
    const tc = v.stopFade / RELEASE_TC;
    g.setValueAtTime(Math.exp(-(t - at) / tc), t);
    g.setTargetAtTime(0, t, tc);
    n.src.stop(end + MIN_FADE);
  }
  /** mb_voice_stop_at: the voice dies away from game time `at`, 60 dB over
   *  `fade` seconds, then stops. A later call replaces one that hasn't begun. */
  stopAt(voice, at, fade) {
    const v = this.voices.get(voice);
    if (!v) return;
    const ctx = this.ctx;
    if (ctx && v.nodes && Number.isFinite(v.stopWhen) && v.stopWhen <= ctx.currentTime) return;
    v.stopAt = at;
    v.stopFade = Number.isFinite(fade) && fade > 0 ? Math.min(fade, 60) : 0;
    v.stopWhen = NaN;
    if (v.nodes) this.applyStop(v);
  }
  /** Stops a voice's nodes but keeps the voice (to be started again). */
  stopNodes(v) {
    const n = v.nodes;
    if (!n) return;
    v.nodes = null;
    n.src.onended = null;
    try {
      n.src.stop();
    } catch {
    }
    n.fadeOut.disconnect();
  }
  update(voice, vol, pan, pitch) {
    const v = this.voices.get(voice);
    if (v) {
      Object.assign(v, { vol, pan, pitch });
      this.apply(v, false);
    }
  }
  apply(v, now) {
    if (!v.nodes || !this.ctx) return;
    const t = this.ctx.currentTime;
    const go = (param, value) => {
      if (now) param.value = value;
      else param.setTargetAtTime(value, t, GLIDE);
    };
    go(v.nodes.gain.gain, Math.min(Math.max(v.vol, 0), 4));
    go(v.nodes.pan.pan, Math.min(Math.max(v.pan, -1), 1));
    go(v.nodes.src.playbackRate, pitchOf(v));
  }
  /** mb_voice_bus: plays the voice into another bus from now on. */
  voiceBus(voice, bus) {
    const v = this.voices.get(voice);
    if (!v || !Number.isInteger(bus) || bus < 0 || bus >= BUS_COUNT || v.bus === bus) return;
    v.bus = bus;
    if (v.nodes) {
      v.nodes.fadeOut.disconnect();
      v.nodes.fadeOut.connect(this.buses.input(bus));
    }
  }
  /** mb_bus_fx: 0, or −2 for an unknown bus or kind. Set at once while
   *  fast-forwarding (only where the run got to matters). */
  busFx(bus, kind, fields) {
    return this.buses.set(bus, kind, fields, this.ff);
  }
  stop(voice) {
    const v = this.voices.get(voice);
    if (!v) return;
    this.voices.delete(voice);
    try {
      v.nodes?.src.stop();
    } catch {
    }
  }
  /** Preview aid (`mb.audio()`): the clock and every voice, for checking alignment. */
  debug() {
    const ctx = this.ctx;
    return {
      state: ctx?.state ?? "none",
      sampleRate: ctx?.sampleRate ?? 0,
      baseLatency: ctx?.baseLatency ?? null,
      outputLatency: ctx?.outputLatency ?? null,
      getOutputTimestamp: typeof ctx?.getOutputTimestamp === "function",
      via: this.via,
      currentTime: ctx?.currentTime ?? 0,
      /** Context seconds − performance.now seconds for the sample being heard. */
      offset: this.offset,
      framePeriodMs: this.period,
      presentMs: this.presentMs,
      gameTime: this.frameGame,
      buses: this.buses.debug(),
      voices: [...this.voices].map(([id, v]) => ({
        id,
        sound: v.sound,
        loop: v.loop,
        bus: v.bus,
        at: v.at,
        from: v.from,
        fadeIn: v.fadeIn,
        stopAt: v.stopAt,
        stopFade: v.stopFade,
        when: v.when,
        start: v.start,
        offset: v.offset,
        playing: v.nodes !== null
      }))
    };
  }
};
function pitchOf(v) {
  return Math.min(Math.max(v.pitch, 0.125), 8);
}

// src/boot.ts
async function loadBoot() {
  if (window.__MB_BOOT) return window.__MB_BOOT;
  const res = await fetch("../game/boot.json");
  if (!res.ok) throw new Error(`no boot config (window.__MB_BOOT or game/boot.json): ${res.status}`);
  return await res.json();
}

// src/input.ts
var Input = class {
  constructor(el, layout, touchDeclared, mouseDeclared) {
    this.layout = layout;
    this.touchDeclared = touchDeclared;
    this.mouseDeclared = mouseDeclared;
    el.addEventListener("pointerdown", (e) => this.down(e));
    el.addEventListener("pointermove", (e) => this.move(e));
    el.addEventListener("pointerup", (e) => this.up(e, false));
    el.addEventListener("pointercancel", (e) => this.up(e, true));
    el.style.touchAction = "none";
    el.addEventListener("contextmenu", (e) => e.preventDefault());
  }
  pending = [];
  /** When each pending event happened (ms, performance.now clock); NaN = at the frame. */
  stamps = [];
  /** pointerId → small id, for pointers the game owns. */
  owned = /* @__PURE__ */ new Map();
  nextId = 0;
  /** The latency probe (SPEC §9 `mb.latency()`); its record for each pending event, while it's on. */
  latency = null;
  probes = [];
  /** Probe records of the events the last `take()` returned. */
  taken = [];
  /** Told which kind of pointer the player is using (the round's play report). */
  activity = null;
  /** A tap at CSS pixels (the native feed forwards the tap that starts play). */
  tapAt(x, y) {
    const l = this.layout();
    const lx = Math.fround((x - l.offsetX) / l.scale);
    const ly = Math.fround((y - l.offsetY) / l.scale);
    const [down, up] = this.touchDeclared || !this.mouseDeclared ? [1 /* TouchDown */, 3 /* TouchUp */] : [7 /* MouseDown */, 9 /* MouseUp */];
    const id = this.nextId++ & 255;
    const now = performance.now();
    this.activity?.used("touch");
    this.pending.push([down, id, 0, lx, ly, 1, 0, 0], [up, id, 0, lx, ly, 0, 0, 0]);
    this.stamps.push(now, now);
  }
  /** Queues an event from another source (keyboard, gamepad), with its
   *  timestamp if it has one (otherwise it counts as happening at the frame). */
  push(e, stamp = NaN) {
    this.pending.push(e);
    this.stamps.push(stamp);
    const probe2 = this.latency?.event(e[0], e[1], stamp);
    if (probe2) this.probes[this.pending.length - 1] = probe2;
  }
  /** Moves queued events to the guest's frame. Call once per frame boundary.
   *  With `frame`, each event gets its lag behind the frame's game time:
   *  whole microseconds, within the frame's dt, never growing along the
   *  queue (so times stay monotonic). Without it (input being dropped) no
   *  lags are computed. */
  take(frame) {
    const events = this.pending;
    const stamps = this.stamps;
    this.pending = [];
    this.stamps = [];
    this.taken = this.probes;
    this.probes = [];
    if (!frame) return events;
    const max = Math.max(0, Math.floor(frame.dt * 1e6));
    let prev = max;
    return events.map((ev, i) => {
      const stamp = stamps[i] ?? NaN;
      let lag = Number.isFinite(stamp) ? Math.round((frame.now - stamp) * 1e3 * frame.scale) : 0;
      lag = Math.min(Math.max(lag, 0), max, prev);
      prev = lag;
      if (lag === 0) return ev;
      const out = ev.slice(0, 8);
      out.push(lag);
      return out;
    });
  }
  asTouch(e) {
    return e.pointerType !== "mouse" || !this.mouseDeclared;
  }
  pointer(kind, id, e) {
    const l = this.layout();
    const x = Math.fround((e.clientX - l.offsetX) / l.scale);
    const y = Math.fround((e.clientY - l.offsetY) / l.scale);
    this.push([kind, id, 0, x, y, Math.fround(e.pressure), 0, 0], e.timeStamp);
  }
  down(e) {
    const l = this.layout();
    if (e.clientY >= l.height - l.stripHeight) return;
    const touch = this.asTouch(e);
    if (touch ? !this.touchDeclared : !this.mouseDeclared) return;
    const id = touch ? this.nextId++ & 255 : e.button & 255;
    this.activity?.used(e.pointerType === "mouse" ? "mouse" : "touch");
    this.owned.set(e.pointerId, id);
    this.lastMove.set(e.pointerId, { t: e.timeStamp, x: e.clientX, y: e.clientY });
    e.target.setPointerCapture?.(e.pointerId);
    this.pointer(touch ? 1 /* TouchDown */ : 7 /* MouseDown */, id, e);
  }
  move(e) {
    const id = this.owned.get(e.pointerId);
    let kind;
    if (id !== void 0) kind = this.asTouch(e) ? 2 /* TouchMove */ : 8 /* MouseMove */;
    else if (e.pointerType === "mouse" && this.mouseDeclared) kind = 8 /* MouseMove */;
    else return;
    for (const s of this.samples(e)) this.pointer(kind, id ?? 0, s);
  }
  /** Last move sample delivered per pointer: time (ms) and position. */
  lastMove = /* @__PURE__ */ new Map();
  /** A move's samples. Browsers send at most one pointermove per animation
   *  frame, so on a touch panel faster than the display (120 Hz and up)
   *  samples would be dropped and the one kept would carry a frame-aligned
   *  time; `getCoalescedEvents()` has every sample since the last
   *  pointermove, each with its own time and position. In order, skipping
   *  repeats and anything older than the last sample delivered. */
  samples(e) {
    const all = typeof e.getCoalescedEvents === "function" ? e.getCoalescedEvents() : [];
    const list = all.length ? all : [e];
    const out = [];
    let last = this.lastMove.get(e.pointerId);
    for (const s of list) {
      if (last && (s.timeStamp < last.t || s.timeStamp === last.t && s.clientX === last.x && s.clientY === last.y)) continue;
      last = { t: s.timeStamp, x: s.clientX, y: s.clientY };
      out.push(s);
    }
    if (last) this.lastMove.set(e.pointerId, last);
    return out;
  }
  up(e, cancel) {
    this.lastMove.delete(e.pointerId);
    const id = this.owned.get(e.pointerId);
    if (id === void 0) return;
    this.owned.delete(e.pointerId);
    const kind = this.asTouch(e) ? cancel ? 4 /* TouchCancel */ : 3 /* TouchUp */ : 9 /* MouseUp */;
    this.pointer(kind, id, e);
  }
};
function writeEvents(view, ptr, events, times) {
  events.forEach((ev, i) => {
    const o = ptr + i * 32;
    view.setUint8(o, ev[0]);
    view.setUint8(o + 1, ev[1]);
    view.setUint16(o + 2, ev[2], true);
    view.setFloat32(o + 4, ev[3], true);
    view.setFloat32(o + 8, ev[4], true);
    view.setFloat32(o + 12, ev[5], true);
    view.setFloat32(o + 16, ev[6], true);
    view.setUint32(o + 20, ev[7], true);
    view.setFloat64(o + 24, times[i] ?? 0, true);
  });
}

// src/stats.ts
var KEEP = 600;
var BUDGETS = { draws: 200, triangles: 15e4, skinned: 16, particles: 5e3 };
function budgetUse(f) {
  const n = (k) => typeof f[k] === "number" ? f[k] : 0;
  return { draws: n("draws") + n("reflected") + n("under"), triangles: n("triangles") + n("reflectedTriangles"), skinned: n("skinned"), particles: n("particles") };
}
function overBudget(f) {
  const use = budgetUse(f);
  return Object.keys(BUDGETS).filter((k) => use[k] > BUDGETS[k]);
}
function spread(values) {
  const v = values.filter((x) => typeof x === "number" && Number.isFinite(x)).sort((a, b) => a - b);
  if (v.length === 0) return null;
  const at = (p) => v[Math.min(v.length - 1, Math.floor(p * v.length))];
  return { p50: at(0.5), p95: at(0.95), max: v[v.length - 1] };
}
var Stats = class {
  /** Ring buffer: `next` is where the next frame goes. */
  ring = [];
  next = 0;
  /** Keep what each frame drew (a `host_stats()` call per frame). Always in
   *  the preview; on a device, from the first `mb.stats()` on. */
  drawn;
  constructor(drawn) {
    this.drawn = drawn;
  }
  push(s) {
    this.ring[this.next] = s;
    this.next = (this.next + 1) % KEEP;
  }
  /** The last `n` frames kept, oldest first. */
  frames(n = KEEP) {
    const all = this.ring.length < KEEP ? this.ring.slice() : [...this.ring.slice(this.next), ...this.ring.slice(0, this.next)];
    return all.slice(Math.max(0, all.length - n));
  }
  clear() {
    this.ring = [];
    this.next = 0;
  }
  /** The frames kept and `{ p50, p95, max }` of each numeric field over them. */
  report(opts) {
    const frames = this.frames();
    const keys = /* @__PURE__ */ new Set(["interval", "mb_update", "mb_render", "present", "gpu"]);
    for (const f of frames) for (const k in f) if (k !== "frame" && (typeof f[k] === "number" || f[k] === null)) keys.add(k);
    const summary = {};
    for (const k of keys) summary[k] = spread(frames.map((f) => f[k]));
    if (opts?.reset) this.clear();
    return { frames, summary };
  }
};
function drawStatsOverlay(stats) {
  const el = document.createElement("div");
  el.style.cssText = "position:fixed;top:4px;right:4px;z-index:11;pointer-events:none;padding:3px 5px;border-radius:4px;background:rgba(0,0,0,.45);color:#fff;font:10px/1.3 ui-monospace,Menlo,monospace;white-space:pre;text-align:right";
  document.body.append(el);
  const num = (v) => typeof v === "number" ? v : null;
  const show = () => {
    const frames = stats.frames(60);
    const last = frames[frames.length - 1];
    const interval = spread(frames.map((f) => f.interval));
    const fps = interval ? (1e3 / interval.p50).toFixed(0) : "\u2013";
    const p95 = (k2) => spread(frames.map((f) => f[k2]))?.p95.toFixed(1) ?? "\u2013";
    const draws = num(last?.draws);
    const tris = num(last?.triangles);
    const scale = num(last?.renderScale);
    const k = (v) => v >= 1e3 ? `${(v / 1e3).toFixed(1).replace(/\.0$/, "")}k` : `${v}`;
    const over = last ? overBudget(last) : [];
    const use = last ? budgetUse(last) : null;
    const worst = last?.top?.[0];
    el.style.background = over.length ? "rgba(190,20,20,.85)" : "rgba(0,0,0,.45)";
    el.textContent = `${fps} fps  p95 ${interval ? interval.p95.toFixed(1) : "\u2013"} ms
upd ${p95("mb_update")} ren ${p95("mb_render")} host ${p95("present")}
${draws === null ? "\u2013" : `${draws}+${num(last?.draws2d) ?? 0}`} draws ${tris === null ? "0" : k(tris)} tris` + (scale === null ? "" : ` \xD7${scale.toFixed(2)}`) + (use ? over.map((b) => `
over: ${b} ${k(use[b])} > ${k(BUDGETS[b])}`).join("") : "") + (over.length && worst ? `
most: node ${worst.node} ${k(worst.triangles + worst.reflectedTriangles)} tris` : "");
  };
  show();
  setInterval(show, 250);
}

// src/latency.ts
var KEEP2 = 2e3;
var KEEP_FRAMES = 1200;
var KINDS = {
  1: ["down", "touch"],
  2: ["move", "touch"],
  3: ["up", "touch"],
  4: ["cancel", "touch"],
  5: ["key_down", "key"],
  6: ["key_up", "key"],
  7: ["down", "touch"],
  8: ["move", "touch"],
  9: ["up", "touch"],
  10: ["wheel", "touch"]
};
var Latency = class {
  enabled = false;
  ring = [];
  next = 0;
  frameRing = [];
  nextFrame = 0;
  /** The live frame being run (set by `deliver`), until its submit. */
  running = null;
  /** Delivered by the frame being run, waiting for its submit. */
  current = [];
  /** The newest tilt sample not yet delivered (the game sees only the newest one per frame). */
  tiltPending = null;
  /** An input event reached the page; null while off or when it has no time of its own. */
  event(kind, id, stamp) {
    const k = KINDS[kind];
    if (!this.enabled || !k || !Number.isFinite(stamp)) return null;
    return { kind: k[0], group: k[1], id: k[1] === "touch" ? id : null, stamp, received: performance.now(), frame: null, frameStart: null, submitted: null, presented: null };
  }
  /** A motion sample (tilt, or attitude for games that aim with it) reached the page at
   *  `received`, taken at `stamp`. */
  tilt(stamp, received, kind = "tilt") {
    if (!this.enabled) return;
    this.tiltPending = { kind, group: "tilt", id: null, stamp, received, frame: null, frameStart: null, submitted: null, presented: null };
  }
  /** A live frame delivered these events (and the newest tilt sample) to the game. */
  deliver(events, frame, frameStart) {
    if (!this.enabled) return;
    this.current = [];
    this.running = { frame, frameStart };
    const all = this.tiltPending ? [...events, this.tiltPending] : events;
    this.tiltPending = null;
    for (const e of all) {
      if (!e) continue;
      e.frame = frame;
      e.frameStart = frameStart;
      this.current.push(e);
      this.ring[this.next] = e;
      this.next = (this.next + 1) % KEEP2;
    }
  }
  /** The frame just run was submitted to the GPU at `t` (null: it wasn't presented). */
  submitted(t) {
    for (const e of this.current) e.submitted = t;
    this.current = [];
    if (this.running && t !== null) {
      this.frameRing[this.nextFrame] = { ...this.running, submitted: t, presented: null };
      this.nextFrame = (this.nextFrame + 1) % KEEP_FRAMES;
    }
    this.running = null;
  }
  /** Native: session frame `frame` was presented at `t` (performance.now clock). */
  presented(frame, t) {
    if (!Number.isFinite(frame) || !Number.isFinite(t)) return;
    for (let j = 1; j <= this.frameRing.length; j++) {
      const f = this.frameRing[(this.nextFrame - j + this.frameRing.length) % this.frameRing.length];
      if (f.frame < frame) break;
      if (f.frame === frame) f.presented ??= t;
    }
    const n = this.ring.length;
    for (let j = 1; j <= n; j++) {
      const e = this.ring[(this.next - j + n) % n];
      if (e.frame === null) continue;
      if (e.frame < frame) break;
      if (e.frame === frame && e.presented === null) e.presented = t;
    }
  }
  /** Kept events, oldest first. */
  events() {
    return this.ring.length < KEEP2 ? this.ring.slice() : [...this.ring.slice(this.next), ...this.ring.slice(0, this.next)];
  }
  /** Kept frames, oldest first. */
  frames() {
    const r = this.frameRing;
    return r.length < KEEP_FRAMES ? r.slice() : [...r.slice(this.nextFrame), ...r.slice(0, this.nextFrame)];
  }
  clear() {
    this.ring = [];
    this.next = 0;
    this.frameRing = [];
    this.nextFrame = 0;
    this.running = null;
    this.current = [];
    this.tiltPending = null;
  }
  /** The events and, per group, p50/p95/max of each stage (ms):
   *  deliver = received − stamp, toFrame = frameStart − received,
   *  toSubmit = submitted − stamp, toPresent = presented − stamp. */
  report() {
    const events = this.events();
    const summary = {};
    for (const g of ["touch", "key", "tilt"]) {
      const es = events.filter((e) => e.group === g);
      if (!es.length) continue;
      const d = (f) => spread(es.map(f));
      summary[g] = {
        deliver: d((e) => e.received - e.stamp),
        toFrame: d((e) => e.frameStart === null ? null : e.frameStart - e.received),
        toSubmit: d((e) => e.submitted === null ? null : e.submitted - e.stamp),
        toPresent: d((e) => e.presented === null ? null : e.presented - e.stamp)
      };
    }
    return { enabled: this.enabled, events, frames: this.frames(), summary };
  }
};

// src/motion.ts
var SAMPLE_HZ = 100;
var q = (v, steps, lo, hi) => Math.round(Math.max(lo, Math.min(hi, v)) * steps) / steps;
var vec = (x, y, z, max) => [x, y, z].every(Number.isFinite) ? [q(x, 4096, -max, max), q(y, 4096, -max, max), q(z, 4096, -max, max)] : null;
function att(a) {
  if (a.length < 7 || !a.slice(0, 7).every(Number.isFinite)) return null;
  const n = Math.hypot(a[0], a[1], a[2], a[3]);
  if (n < 0.5) return null;
  const s = a[3] < 0 ? -1 / n : 1 / n;
  const c = (v) => q(v * s, 32768, -1, 1);
  const r = (v) => q(v, 1024, -64, 64);
  return [c(a[0]), c(a[1]), c(a[2]), c(a[3]), r(a[4]), r(a[5]), r(a[6])];
}
var qmul = (a, b) => [
  a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
  a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
  a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
  a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2]
];
var qaxis = (x, y, z, angle) => {
  const s = Math.sin(angle / 2);
  return [x * s, y * s, z * s, Math.cos(angle / 2)];
};
function pose(yaw, pitch, roll) {
  return qmul(qmul(qaxis(0, -1, 0, yaw), qaxis(1, 0, 0, -pitch)), qaxis(0, 0, 1, roll));
}
function gravityOf(q2) {
  const [x, y, z, w] = q2;
  return [2 * (x * y + w * z), 1 - 2 * (x * x + z * z), 2 * (y * z - w * x)];
}
function rateOf(from, to, dt) {
  let d = qmul([-from[0], -from[1], -from[2], from[3]], to);
  if (d[3] < 0) d = [-d[0], -d[1], -d[2], -d[3]];
  return dt > 0 ? [2 * d[0] / dt, 2 * d[1] / dt, 2 * d[2] / dt] : [0, 0, 0];
}
var Sensors = class {
  /** `source`: who supplies samples: the iOS app pushes them ("native"), a phone's browser has
   *  its own events ("device"), a desktop preview emulates them. `off`: start with tilt and
   *  motion off (the preview's `?sensors=none`). */
  constructor(want, source, off = false) {
    this.want = want;
    this.available = !off;
    if (source === "emulate") this.emulate();
    else if (source === "device") this.device();
  }
  /** Latest samples; null until the first one arrives. */
  tilt = null;
  motion = null;
  attitude = null;
  /** 0–1, or −1 when the mic is unavailable (denied, missing). */
  loudness = null;
  /** False while tilt, motion and attitude are off (`off()`): no samples reach the game. */
  available = true;
  /** A touch device whose browser has orientation and motion events (a phone, a tablet). */
  static deviceHasMotion() {
    return typeof DeviceOrientationEvent !== "undefined" && navigator.maxTouchPoints > 0 && matchMedia("(pointer: coarse)").matches;
  }
  /** Preview: tilt and motion were set from outside, and the pointer leaves them alone until `on()`. */
  pinnedMotion = false;
  /** Native: gravity and user acceleration, game axes, in g, and the attitude (quaternion and
   *  rotation rate) when there is one. A sample turns the sensors back on (and in the preview
   *  pins them). */
  set(gx, gy, gz, ax, ay, az, attitude = []) {
    this.available = true;
    this.pinnedMotion = this.emulated;
    if (this.want.tilt) this.tilt = vec(gx, gy, gz, 4) ?? this.tilt;
    if (this.want.motion) this.motion = vec(ax, ay, az, 16) ?? this.motion;
    if (this.want.attitude) this.attitude = att(attitude) ?? this.attitude;
  }
  /** No tilt or motion from now on (the game sees them as unavailable, -6). */
  off() {
    this.available = false;
    this.tilt = null;
    this.motion = null;
    this.attitude = null;
  }
  /** Tilt and motion back on (or unpinned): in the preview, emulated again
   *  from upright and still; natively, from the next sample. */
  on() {
    if (this.available && !this.pinnedMotion) return;
    this.available = true;
    this.pinnedMotion = false;
    if (this.emulated) this.seed();
  }
  /** The preview's starting readings: upright and still. */
  seed() {
    if (!this.available) return;
    if (this.want.tilt) this.tilt = [0, 1, 0];
    if (this.want.motion) this.motion = [0, 0, 0];
    if (this.want.attitude) this.attitude = [0, 0, 0, 1, 0, 0, 0];
    this.pointer.tilt = null;
    this.pointer.motion = null;
    this.pointer.pose = null;
  }
  /** Preview: tilt, motion and attitude as the pointer has them now, waiting for the next sample. */
  pointer = { tilt: null, motion: null, pose: null };
  setLoudness(v) {
    if (!this.want.loudness || !Number.isFinite(v)) return;
    this.loudness = v < 0 ? -1 : q(v, 1024, 0, 1);
  }
  /** Preview: a console-pinned loudness that the emulation leaves alone (undefined unpins). */
  pinned;
  /** `mb.loudness()` from outside: native samples, or a preview pin. */
  external(v) {
    if (this.emulated) {
      this.pinned = v;
      if (v !== void 0) this.setLoudness(v);
    } else if (v !== void 0) {
      this.setLoudness(v);
    }
  }
  emulated = false;
  /** Preview: an emulated tilt sample was taken at `stamp` (for the latency probe). */
  onEmulatedTilt = null;
  emulate() {
    this.emulated = true;
    this.seed();
    const pointer = this.pointer;
    let last = null;
    window.addEventListener("pointermove", (e) => {
      if (!this.available || this.pinnedMotion) return;
      if (this.want.attitude) {
        const angle = (pos, size) => Math.max(-0.5, Math.min(0.5, pos / size - 0.5)) * (Math.PI / 2);
        pointer.pose = pose(angle(e.clientX, window.innerWidth), -angle(e.clientY, window.innerHeight), 0);
        if (this.want.tilt) pointer.tilt = vec(...gravityOf(pointer.pose), 4);
      } else {
        const angle = (pos, size) => Math.max(-1, Math.min(1, (pos / size - 0.5) * 3)) * (Math.PI / 2);
        const roll = angle(e.clientX, window.innerWidth);
        const pitch = angle(e.clientY, window.innerHeight);
        if (this.want.tilt) pointer.tilt = vec(Math.sin(roll) * Math.cos(pitch), Math.cos(roll) * Math.cos(pitch), -Math.sin(pitch), 4);
      }
      const t = e.timeStamp / 1e3;
      if (last && t > last.t) {
        const dt = t - last.t;
        const vx = (e.clientX - last.x) / dt;
        const vy = (e.clientY - last.y) / dt;
        const k = 6e-3;
        if (this.want.motion) pointer.motion = vec((vx - last.vx) * k, (vy - last.vy) * k, 0, 16);
        last = { x: e.clientX, y: e.clientY, vx, vy, t };
      } else {
        last = { x: e.clientX, y: e.clientY, vx: 0, vy: 0, t };
      }
    });
    if (this.want.motion) {
      setInterval(() => {
        const m = pointer.motion;
        if (m) pointer.motion = vec(m[0] * 0.5, m[1] * 0.5, m[2] * 0.5, 16);
      }, 50);
    }
    if (this.want.tilt || this.want.motion || this.want.attitude) {
      let last2 = null;
      setInterval(() => {
        if (!this.available || this.pinnedMotion) return;
        let sampled = false;
        if (pointer.tilt && pointer.tilt.join() !== this.tilt?.join()) {
          this.tilt = pointer.tilt;
          sampled = true;
        }
        if (pointer.motion) this.motion = pointer.motion;
        if (this.want.attitude && pointer.pose) {
          const a = att([...pointer.pose, ...rateOf(last2 ?? pointer.pose, pointer.pose, 1 / SAMPLE_HZ)]);
          last2 = pointer.pose;
          if (a && a.join() !== this.attitude?.join()) {
            this.attitude = a;
            sampled = true;
          }
        }
        if (sampled) this.onEmulatedTilt?.(performance.now());
      }, 1e3 / SAMPLE_HZ);
    }
    if (this.want.loudness) this.emulateMic();
  }
  /** A phone's browser. iOS delivers these events only once the page has asked
   *  for permission from a tap (`DeviceMotionEvent.requestPermission()`, which
   *  the web player does when play starts); until then the game sees −6.
   *  Attitude isn't read from them yet, so a game that declares it sees it as
   *  unavailable (−6) in a phone's browser. */
  device() {
    const G = 9.80665;
    const toGame = (x, y, z, max) => {
      const a = (screen.orientation?.angle ?? 0) * Math.PI / 180;
      const sx = x * Math.cos(a) + y * Math.sin(a);
      const sy = -x * Math.sin(a) + y * Math.cos(a);
      return vec(sx, -sy, z, max);
    };
    if (this.want.tilt) {
      window.addEventListener("deviceorientation", (e) => {
        if (!this.available || e.beta === null || e.gamma === null) return;
        const b = e.beta * Math.PI / 180;
        const g = e.gamma * Math.PI / 180;
        this.tilt = toGame(Math.cos(b) * Math.sin(g), -Math.sin(b), -Math.cos(b) * Math.cos(g), 4) ?? this.tilt;
      });
    }
    if (this.want.motion) {
      window.addEventListener("devicemotion", (e) => {
        const a = e.acceleration;
        if (!this.available || a?.x == null || a.y == null || a.z == null) return;
        this.motion = toGame(a.x / G, a.y / G, a.z / G, 16) ?? this.motion;
      });
    }
    if (this.want.loudness) this.emulateMic();
  }
  /** Desktop preview: the real microphone if the browser allows it, else keys (see top). */
  emulateMic() {
    this.loudness = 0;
    let held = null;
    let mic = null;
    let level = 0;
    const keyLevel = (e) => e.code === "Space" ? 0.85 : /^Digit[0-9]$/.test(e.code) ? Number(e.code.slice(5)) / 10 : null;
    window.addEventListener("keydown", (e) => {
      const k = keyLevel(e);
      if (k !== null) held = k;
    });
    window.addEventListener("keyup", (e) => {
      if (keyLevel(e) !== null) held = null;
    });
    const start = async () => {
      window.removeEventListener("pointerdown", start);
      try {
        const stream = await navigator.mediaDevices.getUserMedia({ audio: { echoCancellation: false, autoGainControl: false, noiseSuppression: false } });
        const ctx = new AudioContext();
        mic = ctx.createAnalyser();
        mic.fftSize = 1024;
        ctx.createMediaStreamSource(stream).connect(mic);
        console.log("[mb] preview: loudness from the microphone (or hold 1\u20139 / Space)");
      } catch {
        console.log("[mb] preview: no microphone; hold 1\u20139 for 0.1\u20130.9, Space for a blow (0.85)");
      }
    };
    window.addEventListener("pointerdown", start);
    const buf = new Float32Array(1024);
    setInterval(() => {
      if (this.pinned !== void 0) return;
      let target = held ?? 0;
      if (mic) target = Math.max(target, levelOf(mic, buf));
      level += (target - level) * (target > level ? 0.6 : 0.15);
      this.setLoudness(level);
    }, 16);
  }
};
function levelOf(a, buf) {
  a.getFloatTimeDomainData(buf);
  let s = 0;
  for (const v of buf) s += v * v;
  const db = 10 * Math.log10(s / buf.length + 1e-12);
  return Math.max(0, Math.min(1, (db + 60) / 60));
}

// src/keys.ts
var HID = {
  Enter: 40,
  Escape: 41,
  Backspace: 42,
  Tab: 43,
  Space: 44,
  Minus: 45,
  Equal: 46,
  BracketLeft: 47,
  BracketRight: 48,
  Backslash: 49,
  Semicolon: 51,
  Quote: 52,
  Backquote: 53,
  Comma: 54,
  Period: 55,
  Slash: 56,
  CapsLock: 57,
  ArrowRight: 79,
  ArrowLeft: 80,
  ArrowDown: 81,
  ArrowUp: 82,
  ControlLeft: 224,
  ShiftLeft: 225,
  AltLeft: 226,
  MetaLeft: 227,
  ControlRight: 228,
  ShiftRight: 229,
  AltRight: 230,
  MetaRight: 231
};
for (let i = 0; i < 26; i++) HID[`Key${String.fromCharCode(65 + i)}`] = 4 + i;
for (let i = 1; i <= 9; i++) HID[`Digit${i}`] = 29 + i;
HID.Digit0 = 39;
for (let i = 1; i <= 12; i++) HID[`F${i}`] = 57 + i;
var KEY_DOWN = 5;
var KEY_UP = 6;
var PAD_BUTTON = 11;
var PAD_AXIS = 12;
var PAD_START = 9;
var DPAD = [12, 13, 14, 15];
var TRIGGERS = [6, 7];
var TRIGGER_DOWN = 0.2;
var TRIGGER_UP = 0.1;
function buttonDown(i, v, was) {
  if (TRIGGERS.includes(i)) return was ? v > TRIGGER_UP : v >= TRIGGER_DOWN;
  return v > 0.5;
}
var PAD_STYLES = ["generic", "xbox", "playstation", "nintendo"];
function padStyle(id) {
  const s = id.toLowerCase();
  if (/dualsense|dualshock|playstation|ps[345]\b|054c|backbone.*playstation/.test(s)) return 2 /* PlayStation */;
  if (/xbox|xinput|045e|backbone|kishi|gamesir/.test(s)) return 1 /* Xbox */;
  if (/nintendo|switch|joy-con|pro controller|057e/.test(s)) return 3 /* Nintendo */;
  return 0 /* Generic */;
}
function padName(id) {
  return id.replace(/\s*\(.*\)\s*$/, "").replace(/^[0-9a-f]{4}-[0-9a-f]{4}-/i, "").trim().slice(0, 64) || "Controller";
}
var Keyboard = class {
  constructor(push, activity, dom) {
    this.push = push;
    this.activity = activity;
    if (!dom) return;
    const on = (kind) => (e) => {
      const code = HID[e.code];
      if (code === void 0 || e.repeat) return;
      e.preventDefault();
      this.key(kind, code, e.timeStamp);
    };
    window.addEventListener("keydown", on(KEY_DOWN));
    window.addEventListener("keyup", on(KEY_UP));
  }
  /** Off while the game is only a feed card (`mb.inputs(false)`): no keys reach it. */
  enabled = true;
  /** Keys down as the game last heard. */
  held = /* @__PURE__ */ new Set();
  /** A key went down (`KEY_DOWN`) or up, by HID usage: the DOM's, or native's (`mb.key`). */
  key(kind, code, stamp) {
    if (!this.enabled || kind !== KEY_DOWN && kind !== KEY_UP || !(code > 0 && code < 256)) return;
    if (kind === KEY_DOWN) this.held.add(code);
    else this.held.delete(code);
    this.activity?.used("keyboard");
    this.push([kind, 0, 0, 0, 0, 0, 0, code], stamp);
  }
  /** Key-ups for every key the game still thinks is down (input turning off:
   *  a key let go in browse would otherwise stay down in the game). */
  releaseAll() {
    const out = [...this.held].map((code) => [KEY_UP, 0, 0, 0, 0, 0, 0, code]);
    this.held.clear();
    return out;
  }
};
var Gamepads = class {
  constructor(native2, activity) {
    this.activity = activity;
    this.native = native2 ? [] : null;
  }
  last = /* @__PURE__ */ new Map();
  /** Native's latest controller state (boot `nativePads`); null: the Gamepad API. */
  native;
  /** The pad that last changed, for rumble (web). */
  recent = null;
  /** Off while the game is only a feed card (`mb.inputs(false)`): controllers don't reach it. */
  enabled = true;
  /** Presses the host hands over (`mb.padTap`), delivered at the next poll. */
  taps = [];
  /** Browse (off) or play (on). Turned on, the game hears the directions held
   *  right now (the d-pad and the sticks: a player still holding left when
   *  play resumes keeps walking left), but buttons held now count as already
   *  seen, so the press that entered or resumed play doesn't act in the game
   *  too; that press comes through `tap` when it should. */
  setEnabled(on) {
    if (on && !this.enabled) {
      this.last.clear();
      for (const pad of this.read()) {
        const now = this.normalize(pad);
        this.last.set(pad.index, {
          buttons: now.buttons.map((v, i) => DPAD.includes(i) ? 0 : buttonDown(i, v, false) ? 1 : 0),
          axes: now.axes.map(() => 0)
        });
      }
    }
    this.enabled = on;
  }
  /** Releases for every button the game still thinks is held and every axis
   *  off center (input turning off: what's let go in browse would otherwise
   *  stay held in the game). */
  releaseAll() {
    const out = [];
    for (const [index, st] of this.last) {
      const style = st.style ?? 0;
      st.buttons.forEach((v, i) => {
        if (v > 0.5) out.push([PAD_BUTTON, style, index, 0, 0, 0, 0, i]);
      });
      st.axes.forEach((v, i) => {
        if (v !== 0) out.push([PAD_AXIS, style, index, 0, 0, 0, 0, i]);
      });
      this.last.set(index, { buttons: st.buttons.map(() => 0), axes: st.axes.map(() => 0), style });
    }
    return out;
  }
  /** The host's controller press that entered play (as `mb.tap` hands over a
   *  touch): down and up of `button` on pad `index`, at the next frame. */
  tap(button, index) {
    if (!(button >= 0 && button < 32 && button !== PAD_START && index >= 0 && index < 16)) return;
    const pad = this.read().find((p) => p.index === index);
    const style = Math.max(0, PAD_STYLES.indexOf(pad?.style ?? "generic"));
    this.taps.push([PAD_BUTTON, style, index, 0, 0, 1, 0, button], [PAD_BUTTON, style, index, 0, 0, 0, 0, button]);
    this.recent = index;
    this.activity?.used("gamepad", { style, name: String(pad?.name ?? "Controller").slice(0, 64) });
  }
  /** Every connected controller's state: native's, or the Gamepad API's. */
  read() {
    return this.native ?? [...navigator.getGamepads?.() ?? []].flatMap(
      (p) => p ? [{ index: p.index, style: PAD_STYLES[padStyle(p.id)], name: padName(p.id), buttons: p.buttons.map((b) => b.value), axes: [...p.axes] }] : []
    );
  }
  normalize(pad) {
    return {
      buttons: pad.buttons.slice(0, 32).map((b) => Math.fround(Number(b) || 0)),
      axes: pad.axes.slice(0, 8).map((a) => Math.fround(Math.abs(Number(a) || 0) < 0.1 ? 0 : Number(a)))
    };
  }
  /** Native: every connected controller's state now. */
  setNative(pads) {
    if (this.native !== null) this.native = pads;
  }
  poll(push) {
    for (const e of this.taps.splice(0)) push(e);
    if (!this.enabled) return;
    for (const pad of this.read()) {
      if (!(pad.index >= 0 && pad.index < 16)) continue;
      const style = Math.max(0, PAD_STYLES.indexOf(pad.style));
      const prev = this.last.get(pad.index) ?? { buttons: [], axes: [] };
      const { buttons, axes } = this.normalize(pad);
      let changed = false;
      const down = buttons.map((v, i) => buttonDown(i, v, (prev.buttons[i] ?? 0) > 0.5) ? 1 : 0);
      down.forEach((d, i) => {
        if (i === PAD_START) return;
        if (d !== ((prev.buttons[i] ?? 0) > 0.5 ? 1 : 0)) {
          const v = buttons[i] ?? 0;
          push([PAD_BUTTON, style, pad.index, 0, 0, TRIGGERS.includes(i) ? d : v, 0, i]);
          changed = true;
        }
      });
      axes.forEach((v, i) => {
        if (Math.abs(v - (prev.axes[i] ?? 0)) > 0.02) {
          push([PAD_AXIS, style, pad.index, 0, 0, v, 0, i]);
          changed = true;
        }
      });
      this.last.set(pad.index, { buttons: down, axes, style });
      if (changed) {
        this.recent = pad.index;
        this.activity?.used("gamepad", { style, name: String(pad.name).slice(0, 64) });
      }
    }
  }
  /** Web: a SPEC §5.7 haptic (0 light … 3 heavy) as a rumble on the pad in
   *  use, where the browser can. Native rumbles its own controllers. */
  rumble(kind) {
    if (this.native !== null || this.recent === null) return;
    const pad = navigator.getGamepads?.()[this.recent];
    const act = pad?.vibrationActuator;
    const [ms, strong, weak] = [[25, 0, 0.35], [40, 0.3, 0.6], [60, 0.7, 0.9], [90, 1, 1]][kind] ?? [25, 0, 0.35];
    act?.playEffect?.("dual-rumble", { duration: ms, strongMagnitude: strong, weakMagnitude: weak })?.catch(() => {
    });
  }
};

// src/play.ts
var ACTIVE_HOLD = 1.5;
var Play = class {
  constructor(orientation) {
    this.orientation = orientation;
  }
  /** Seconds each input was in use (credited while it's been touched within ACTIVE_HOLD). */
  active = /* @__PURE__ */ new Map();
  held = /* @__PURE__ */ new Map();
  /** When each input was last used, in round seconds. */
  lastUsed = /* @__PURE__ */ new Map();
  pad = null;
  seconds = 0;
  running = false;
  /** Used since the last frame (an input before the round starts still counts: the tap that starts it). */
  pending = /* @__PURE__ */ new Set();
  used(input, pad) {
    this.pending.add(input);
    if (pad) this.pad = pad;
  }
  /** `mb_round(1)`: a round starts. */
  start() {
    this.active.clear();
    this.held.clear();
    this.lastUsed.clear();
    this.seconds = 0;
    this.running = true;
    for (const i of this.pending) this.lastUsed.set(i, 0);
    this.pending.clear();
  }
  /** A live frame of `dt` game seconds. */
  frame(dt) {
    if (!this.running) return;
    this.seconds += dt;
    for (const i of this.pending) this.lastUsed.set(i, this.seconds);
    this.pending.clear();
    for (const [i, t] of this.lastUsed) {
      if (this.seconds - t <= ACTIVE_HOLD) this.active.set(i, (this.active.get(i) ?? 0) + dt);
    }
    const o = this.orientation();
    this.held.set(o, (this.held.get(o) ?? 0) + dt);
  }
  /** `mb_round(2)`: the round is over; its report, or null if none was running. */
  finish() {
    if (!this.running) return null;
    this.running = false;
    for (const i of this.pending) if (!this.lastUsed.has(i)) this.lastUsed.set(i, this.seconds);
    this.pending.clear();
    for (const i of this.lastUsed.keys()) if (!this.active.has(i)) this.active.set(i, 0);
    const byTime = (m) => [...m.entries()].sort((a, b) => b[1] - a[1]).map(([k]) => k);
    const inputs = byTime(this.active);
    const orientations = byTime(this.held);
    if (orientations.length === 0) orientations.push(this.orientation());
    const round = (s) => Math.round(s * 1e3) / 1e3;
    return {
      seconds: round(this.seconds),
      inputs,
      primaryInput: inputs[0] ?? null,
      orientations,
      primaryOrientation: orientations[0],
      controller: inputs.includes("gamepad") && this.pad ? { style: PAD_STYLES[this.pad.style], name: this.pad.name } : null
    };
  }
};

// src/probe.ts
async function probe() {
  const out = {
    userAgent: navigator.userAgent,
    webgpu: "gpu" in navigator,
    offscreenCanvas: typeof OffscreenCanvas !== "undefined",
    transferControlToOffscreen: "transferControlToOffscreen" in HTMLCanvasElement.prototype,
    crossOriginIsolated: self.crossOriginIsolated
  };
  try {
    out.worker = await new Promise((resolve) => {
      const w = new Worker(new URL("probe-worker.js", import.meta.url), { type: "module" });
      const timer = setTimeout(() => resolve({ error: "timeout" }), 3e3);
      w.onmessage = (e) => {
        clearTimeout(timer);
        resolve(e.data);
        w.terminate();
      };
      w.onerror = (e) => {
        clearTimeout(timer);
        resolve({ error: e.message || "worker failed to start" });
      };
      const c = new OffscreenCanvas(16, 16);
      w.postMessage({ canvas: c }, [c]);
    });
  } catch (e) {
    out.worker = { error: String(e) };
  }
  return out;
}

// src/replay.ts
function heldAtEnd(log2) {
  const touches = /* @__PURE__ */ new Map();
  const mouse = /* @__PURE__ */ new Map();
  const keys = /* @__PURE__ */ new Set();
  const buttons = /* @__PURE__ */ new Map();
  const axes = /* @__PURE__ */ new Map();
  for (const f of log2.frames) {
    for (const e of f.events ?? []) {
      const [kind, id, pad, x, y, a, , code] = e;
      if (kind === 1 || kind === 2) touches.set(id, [4, id, pad, x, y, 0, 0, code]);
      else if (kind === 3 || kind === 4) touches.delete(id);
      else if (kind === 5) keys.add(code);
      else if (kind === 6) keys.delete(code);
      else if (kind === 7) mouse.set(code, [9, id, pad, x, y, 0, 0, code]);
      else if (kind === 8) for (const [b, m] of mouse) mouse.set(b, [9, m[1], m[2], x, y, 0, 0, b]);
      else if (kind === 9) mouse.delete(code);
      else if (kind === 11) {
        if (a > 0.5) buttons.set(`${pad}:${code}`, [11, id, pad, 0, 0, 0, 0, code]);
        else buttons.delete(`${pad}:${code}`);
      } else if (kind === 12) {
        if (a !== 0) axes.set(`${pad}:${code}`, [12, id, pad, 0, 0, 0, 0, code]);
        else axes.delete(`${pad}:${code}`);
      }
    }
  }
  return [...touches.values(), ...mouse.values(), ...[...keys].map((code) => [6, 0, 0, 0, 0, 0, 0, code]), ...buttons.values(), ...axes.values()];
}
var Recorder = class {
  log;
  constructor(header) {
    this.log = { version: 1, ...header, frames: [] };
  }
  frame(dt, events, assets, sensors, calls = [], lost = 0) {
    const f = { dt, ...sensors };
    if (lost > 0) f.lost = lost;
    if (events.length) f.events = events;
    if (assets.length) f.assets = assets;
    if (calls.length) f.calls = calls;
    this.log.frames.push(f);
  }
};

// src/abi.json
var abi_default = {
  abi: 0,
  imports: [
    {
      name: "mb_log",
      params: [
        "i32",
        "i32",
        "i32"
      ],
      requires: null,
      results: []
    },
    {
      name: "mb_mark",
      params: [
        "i32",
        "i32"
      ],
      requires: null,
      results: []
    },
    {
      name: "mb_param",
      params: [
        "i32",
        "i32",
        "i32",
        "i32"
      ],
      requires: null,
      results: [
        "i32"
      ]
    },
    {
      name: "mb_time",
      params: [],
      requires: null,
      results: [
        "f64"
      ]
    },
    {
      name: "mb_time_lost",
      params: [],
      requires: null,
      results: [
        "f32"
      ]
    },
    {
      name: "mb_rand_seed",
      params: [],
      requires: null,
      results: [
        "i64"
      ]
    },
    {
      name: "mb_daily_seed",
      params: [],
      requires: null,
      results: [
        "i64"
      ]
    },
    {
      name: "mb_round",
      params: [
        "i32"
      ],
      requires: null,
      results: []
    },
    {
      name: "mb_player_id",
      params: [
        "i32"
      ],
      requires: null,
      results: [
        "i32"
      ]
    },
    {
      name: "mb_locale",
      params: [
        "i32",
        "i32"
      ],
      requires: null,
      results: [
        "i32"
      ]
    },
    {
      name: "mb_screen",
      params: [
        "i32"
      ],
      requires: null,
      results: []
    },
    {
      name: "mb_asset_load",
      params: [
        "i32",
        "i32"
      ],
      requires: null,
      results: [
        "i32"
      ]
    },
    {
      name: "mb_asset_state",
      params: [
        "i32"
      ],
      requires: null,
      results: [
        "i32"
      ]
    },
    {
      name: "mb_asset_read",
      params: [
        "i32",
        "i32",
        "i32"
      ],
      requires: null,
      results: [
        "i32"
      ]
    },
    {
      name: "mb_input_poll",
      params: [
        "i32",
        "i32"
      ],
      requires: null,
      results: [
        "i32"
      ]
    },
    {
      name: "mb2d_image",
      params: [
        "i32"
      ],
      requires: {
        stdlib: "mb2d",
        version: 1
      },
      results: [
        "i32"
      ]
    },
    {
      name: "mb2d_font",
      params: [
        "i32"
      ],
      requires: {
        stdlib: "mb2d",
        version: 1
      },
      results: [
        "i32"
      ]
    },
    {
      name: "mb2d_clear",
      params: [
        "i32"
      ],
      requires: {
        stdlib: "mb2d",
        version: 1
      },
      results: []
    },
    {
      name: "mb2d_push",
      params: [],
      requires: {
        stdlib: "mb2d",
        version: 1
      },
      results: []
    },
    {
      name: "mb2d_pop",
      params: [],
      requires: {
        stdlib: "mb2d",
        version: 1
      },
      results: []
    },
    {
      name: "mb2d_translate",
      params: [
        "f32",
        "f32"
      ],
      requires: {
        stdlib: "mb2d",
        version: 1
      },
      results: []
    },
    {
      name: "mb2d_rotate",
      params: [
        "f32"
      ],
      requires: {
        stdlib: "mb2d",
        version: 1
      },
      results: []
    },
    {
      name: "mb2d_scale",
      params: [
        "f32",
        "f32"
      ],
      requires: {
        stdlib: "mb2d",
        version: 1
      },
      results: []
    },
    {
      name: "mb2d_blend",
      params: [
        "i32"
      ],
      requires: {
        stdlib: "mb2d",
        version: 1
      },
      results: []
    },
    {
      name: "mb2d_rect",
      params: [
        "f32",
        "f32",
        "f32",
        "f32",
        "i32"
      ],
      requires: {
        stdlib: "mb2d",
        version: 1
      },
      results: []
    },
    {
      name: "mb2d_rect_gradient",
      params: [
        "f32",
        "f32",
        "f32",
        "f32",
        "i32",
        "i32"
      ],
      requires: {
        stdlib: "mb2d",
        version: 1
      },
      results: []
    },
    {
      name: "mb2d_circle",
      params: [
        "f32",
        "f32",
        "f32",
        "i32"
      ],
      requires: {
        stdlib: "mb2d",
        version: 1
      },
      results: []
    },
    {
      name: "mb2d_line",
      params: [
        "f32",
        "f32",
        "f32",
        "f32",
        "f32",
        "i32"
      ],
      requires: {
        stdlib: "mb2d",
        version: 1
      },
      results: []
    },
    {
      name: "mb2d_poly",
      params: [
        "i32",
        "i32",
        "i32"
      ],
      requires: {
        stdlib: "mb2d",
        version: 1
      },
      results: []
    },
    {
      name: "mb2d_sprite",
      params: [
        "i32",
        "f32",
        "f32",
        "f32",
        "f32",
        "f32",
        "f32",
        "f32",
        "f32",
        "i32"
      ],
      requires: {
        stdlib: "mb2d",
        version: 1
      },
      results: []
    },
    {
      name: "mb2d_text",
      params: [
        "i32",
        "f32",
        "f32",
        "f32",
        "i32",
        "i32",
        "i32"
      ],
      requires: {
        stdlib: "mb2d",
        version: 1
      },
      results: []
    },
    {
      name: "mb2d_measure",
      params: [
        "i32",
        "f32",
        "i32",
        "i32"
      ],
      requires: {
        stdlib: "mb2d",
        version: 1
      },
      results: [
        "f32"
      ]
    },
    {
      name: "mb2d_rrect",
      params: [
        "f32",
        "f32",
        "f32",
        "f32",
        "f32",
        "f32",
        "f32",
        "i32",
        "i32"
      ],
      requires: {
        stdlib: "mb2d",
        version: 1
      },
      results: []
    },
    {
      name: "mb2d_text_style",
      params: [
        "f32",
        "f32",
        "i32",
        "f32"
      ],
      requires: {
        stdlib: "mb2d",
        version: 1
      },
      results: []
    },
    {
      name: "mb2d_antialias",
      params: [
        "i32"
      ],
      requires: {
        stdlib: "mb2d",
        version: 1
      },
      results: []
    },
    {
      name: "mb2d_font_load",
      params: [
        "i32"
      ],
      requires: {
        stdlib: "mb2d",
        version: 2
      },
      results: [
        "i32"
      ]
    },
    {
      name: "mb2d_font_metrics",
      params: [
        "i32",
        "i32"
      ],
      requires: {
        stdlib: "mb2d",
        version: 2
      },
      results: [
        "i32"
      ]
    },
    {
      name: "mb3d_mesh",
      params: [
        "i32",
        "i32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 1
      },
      results: [
        "i32"
      ]
    },
    {
      name: "mb3d_texture",
      params: [
        "i32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 1
      },
      results: [
        "i32"
      ]
    },
    {
      name: "mb3d_material",
      params: [
        "i32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 1
      },
      results: [
        "i32"
      ]
    },
    {
      name: "mb3d_material_set",
      params: [
        "i32",
        "i32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 1
      },
      results: [
        "i32"
      ]
    },
    {
      name: "mb3d_gltf",
      params: [
        "i32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 1
      },
      results: [
        "i32"
      ]
    },
    {
      name: "mb3d_model_spawn",
      params: [
        "i32",
        "i32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 1
      },
      results: [
        "i32"
      ]
    },
    {
      name: "mb3d_node",
      params: [],
      requires: {
        stdlib: "mb3d",
        version: 1
      },
      results: [
        "i32"
      ]
    },
    {
      name: "mb3d_node_parent",
      params: [
        "i32",
        "i32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 1
      },
      results: []
    },
    {
      name: "mb3d_node_transform",
      params: [
        "i32",
        "i32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 1
      },
      results: []
    },
    {
      name: "mb3d_node_mesh",
      params: [
        "i32",
        "i32",
        "i32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 1
      },
      results: []
    },
    {
      name: "mb3d_node_visible",
      params: [
        "i32",
        "i32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 1
      },
      results: []
    },
    {
      name: "mb3d_node_destroy",
      params: [
        "i32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 1
      },
      results: []
    },
    {
      name: "mb3d_camera",
      params: [
        "i32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 1
      },
      results: []
    },
    {
      name: "mb3d_project",
      params: [
        "f32",
        "f32",
        "f32",
        "i32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 1
      },
      results: [
        "i32"
      ]
    },
    {
      name: "mb3d_sun",
      params: [
        "i32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 1
      },
      results: []
    },
    {
      name: "mb3d_light_point",
      params: [
        "i32",
        "f32",
        "f32",
        "f32",
        "f32",
        "f32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 1
      },
      results: [
        "i32"
      ]
    },
    {
      name: "mb3d_sky",
      params: [
        "i32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 1
      },
      results: []
    },
    {
      name: "mb3d_post",
      params: [
        "i32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 1
      },
      results: []
    },
    {
      name: "mb3d_emitter",
      params: [
        "i32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 1
      },
      results: [
        "i32"
      ]
    },
    {
      name: "mb3d_emit",
      params: [
        "i32",
        "f32",
        "f32",
        "f32",
        "f32",
        "f32",
        "f32",
        "i32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 1
      },
      results: []
    },
    {
      name: "mb3d_emit_moving",
      params: [
        "i32",
        "f32",
        "f32",
        "f32",
        "f32",
        "f32",
        "f32",
        "i32",
        "f32",
        "f32",
        "f32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 1
      },
      results: []
    },
    {
      name: "mb3d_trail",
      params: [
        "i32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 1
      },
      results: [
        "i32"
      ]
    },
    {
      name: "mb3d_trail_attach",
      params: [
        "i32",
        "i32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 1
      },
      results: []
    },
    {
      name: "mb3d_trail_detach",
      params: [
        "i32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 1
      },
      results: []
    },
    {
      name: "mb3d_shockwave",
      params: [
        "f32",
        "f32",
        "f32",
        "f32",
        "f32",
        "f32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 1
      },
      results: []
    },
    {
      name: "mb3d_render",
      params: [],
      requires: {
        stdlib: "mb3d",
        version: 1
      },
      results: []
    },
    {
      name: "mb3d_free",
      params: [
        "i32",
        "i32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 2
      },
      results: [
        "i32"
      ]
    },
    {
      name: "mb3d_instances",
      params: [
        "i32",
        "i32",
        "i32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 2
      },
      results: [
        "i32"
      ]
    },
    {
      name: "mb3d_clip_count",
      params: [
        "i32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 2
      },
      results: [
        "i32"
      ]
    },
    {
      name: "mb3d_clip_find",
      params: [
        "i32",
        "i32",
        "i32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 2
      },
      results: [
        "i32"
      ]
    },
    {
      name: "mb3d_clip_duration",
      params: [
        "i32",
        "i32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 2
      },
      results: [
        "f32"
      ]
    },
    {
      name: "mb3d_anim",
      params: [
        "i32",
        "i32",
        "f32",
        "f32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 2
      },
      results: [
        "i32"
      ]
    },
    {
      name: "mb3d_node_find",
      params: [
        "i32",
        "i32",
        "i32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 2
      },
      results: [
        "i32"
      ]
    },
    {
      name: "mb3d_node_morph",
      params: [
        "i32",
        "f32",
        "f32",
        "f32",
        "f32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 2
      },
      results: []
    },
    {
      name: "mb3d_node_world",
      params: [
        "i32",
        "i32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 2
      },
      results: [
        "i32"
      ]
    },
    {
      name: "mb3d_node_material",
      params: [
        "i32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 2
      },
      results: [
        "i32"
      ]
    },
    {
      name: "mb3d_node_fade",
      params: [
        "i32",
        "f32",
        "f32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 2
      },
      results: []
    },
    {
      name: "mb3d_draw_distance",
      params: [
        "f32",
        "f32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 2
      },
      results: []
    },
    {
      name: "mb3d_skin",
      params: [
        "i32",
        "i32",
        "i32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 2
      },
      results: [
        "i32"
      ]
    },
    {
      name: "mb3d_fog",
      params: [
        "i32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 2
      },
      results: []
    },
    {
      name: "mb3d_material_style",
      params: [
        "i32",
        "i32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 2
      },
      results: [
        "i32"
      ]
    },
    {
      name: "mb3d_text",
      params: [
        "i32",
        "i32",
        "i32",
        "i32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 2
      },
      results: [
        "i32"
      ]
    },
    {
      name: "mb3d_dof",
      params: [
        "i32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 2
      },
      results: []
    },
    {
      name: "mb3d_speed",
      params: [
        "i32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 2
      },
      results: []
    },
    {
      name: "mb3d_reflector",
      params: [
        "i32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 2
      },
      results: []
    },
    {
      name: "mb3d_atmosphere",
      params: [
        "i32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 2
      },
      results: []
    },
    {
      name: "mb3d_clouds",
      params: [
        "i32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 2
      },
      results: []
    },
    {
      name: "mb3d_sky_disc",
      params: [
        "i32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 2
      },
      results: []
    },
    {
      name: "mb3d_sky_texture",
      params: [
        "i32",
        "i32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 2
      },
      results: [
        "i32"
      ]
    },
    {
      name: "mb3d_ripple",
      params: [
        "f32",
        "f32",
        "f32",
        "f32",
        "f32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 2
      },
      results: []
    },
    {
      name: "mb3d_wake",
      params: [
        "i32",
        "f32",
        "f32",
        "f32",
        "f32",
        "f32",
        "f32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 2
      },
      results: [
        "i32"
      ]
    },
    {
      name: "mb3d_water",
      params: [
        "i32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 2
      },
      results: []
    },
    {
      name: "mb3d_grade",
      params: [
        "i32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 2
      },
      results: []
    },
    {
      name: "mb3d_shafts",
      params: [
        "i32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 2
      },
      results: []
    },
    {
      name: "mb3d_halos",
      params: [
        "i32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 2
      },
      results: []
    },
    {
      name: "mb3d_film",
      params: [
        "i32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 2
      },
      results: []
    },
    {
      name: "mb3d_material_get",
      params: [
        "i32",
        "i32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 2
      },
      results: [
        "i32"
      ]
    },
    {
      name: "mb3d_model_materials",
      params: [
        "i32",
        "i32",
        "i32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 2
      },
      results: [
        "i32"
      ]
    },
    {
      name: "mb3d_model_node_names",
      params: [
        "i32",
        "i32",
        "i32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 2
      },
      results: [
        "i32"
      ]
    },
    {
      name: "mb3d_retro",
      params: [
        "i32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 2
      },
      results: []
    },
    {
      name: "mb3d_texture_filter",
      params: [
        "i32",
        "i32",
        "i32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 2
      },
      results: [
        "i32"
      ]
    },
    {
      name: "mb3d_backdrop",
      params: [
        "i32",
        "i32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 2
      },
      results: [
        "i32"
      ]
    },
    {
      name: "mb3d_anim_mask",
      params: [
        "i32",
        "i32",
        "f32",
        "f32",
        "i32",
        "i32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 2
      },
      results: [
        "i32"
      ]
    },
    {
      name: "mb3d_anim_take",
      params: [
        "i32",
        "i32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 2
      },
      results: [
        "i32"
      ]
    },
    {
      name: "mb3d_node_get_parent",
      params: [
        "i32"
      ],
      requires: {
        stdlib: "mb3d",
        version: 2
      },
      results: [
        "i32"
      ]
    },
    {
      name: "mb_sound",
      params: [
        "i32"
      ],
      requires: null,
      results: [
        "i32"
      ]
    },
    {
      name: "mb_play",
      params: [
        "i32",
        "f32",
        "f32",
        "f32",
        "i32"
      ],
      requires: null,
      results: [
        "i32"
      ]
    },
    {
      name: "mb_play_at",
      params: [
        "i32",
        "f32",
        "f32",
        "f32",
        "i32",
        "f64"
      ],
      requires: null,
      results: [
        "i32"
      ]
    },
    {
      name: "mb_play_from",
      params: [
        "i32",
        "f32",
        "f32",
        "f32",
        "i32",
        "f64",
        "f64",
        "f32"
      ],
      requires: null,
      results: [
        "i32"
      ]
    },
    {
      name: "mb_voice_set",
      params: [
        "i32",
        "f32",
        "f32",
        "f32"
      ],
      requires: null,
      results: []
    },
    {
      name: "mb_voice_stop",
      params: [
        "i32"
      ],
      requires: null,
      results: []
    },
    {
      name: "mb_voice_stop_at",
      params: [
        "i32",
        "f64",
        "f32"
      ],
      requires: null,
      results: []
    },
    {
      name: "mb_voice_bus",
      params: [
        "i32",
        "i32"
      ],
      requires: null,
      results: []
    },
    {
      name: "mb_bus_fx",
      params: [
        "i32",
        "i32",
        "i32"
      ],
      requires: null,
      results: [
        "i32"
      ]
    },
    {
      name: "mb_tilt",
      params: [
        "i32"
      ],
      requires: {
        sensor: "tilt"
      },
      results: [
        "i32"
      ]
    },
    {
      name: "mb_motion",
      params: [
        "i32"
      ],
      requires: {
        sensor: "motion"
      },
      results: [
        "i32"
      ]
    },
    {
      name: "mb_attitude",
      params: [
        "i32"
      ],
      requires: {
        sensor: "attitude"
      },
      results: [
        "i32"
      ]
    },
    {
      name: "mb_loudness",
      params: [],
      requires: {
        sensor: "loudness"
      },
      results: [
        "f32"
      ]
    },
    {
      name: "mb_light",
      params: [],
      requires: {
        sensor: "light"
      },
      results: [
        "f32"
      ]
    },
    {
      name: "mb_haptic",
      params: [
        "i32"
      ],
      requires: {
        sensor: "haptics"
      },
      results: []
    },
    {
      name: "mb_store_get",
      params: [
        "i32",
        "i32",
        "i32",
        "i32"
      ],
      requires: {
        capability: "store"
      },
      results: [
        "i32"
      ]
    },
    {
      name: "mb_store_set",
      params: [
        "i32",
        "i32",
        "i32",
        "i32"
      ],
      requires: {
        capability: "store"
      },
      results: [
        "i32"
      ]
    },
    {
      name: "mb_score_submit",
      params: [
        "i32",
        "i64"
      ],
      requires: {
        capability: "score"
      },
      results: [
        "i32"
      ]
    },
    {
      name: "mb_score_show",
      params: [
        "i32"
      ],
      requires: {
        capability: "score"
      },
      results: []
    },
    {
      name: "mb_text_begin",
      params: [
        "i32",
        "i32",
        "i32"
      ],
      requires: {
        capability: "text_input"
      },
      results: []
    },
    {
      name: "mb_text_end",
      params: [],
      requires: {
        capability: "text_input"
      },
      results: []
    },
    {
      name: "mb_link_open",
      params: [
        "i32",
        "i32"
      ],
      requires: {
        capability: "links"
      },
      results: [
        "i32"
      ]
    }
  ]
};

// src/abi.ts
var HOST_DIRECT = [
  "mb2d_clear",
  "mb2d_push",
  "mb2d_pop",
  "mb2d_translate",
  "mb2d_rotate",
  "mb2d_scale",
  "mb2d_blend",
  "mb2d_rect",
  "mb2d_rect_gradient",
  "mb2d_circle",
  "mb2d_line",
  "mb2d_sprite",
  "mb2d_rrect",
  "mb2d_text_style",
  "mb2d_antialias",
  "mb3d_model_spawn",
  "mb3d_node",
  "mb3d_node_parent",
  "mb3d_node_mesh",
  "mb3d_node_visible",
  "mb3d_node_destroy",
  "mb3d_light_point",
  "mb3d_emit",
  "mb3d_emit_moving",
  "mb3d_trail_attach",
  "mb3d_trail_detach",
  "mb3d_shockwave",
  "mb3d_render",
  // mb3d 2
  "mb3d_free",
  "mb3d_clip_count",
  "mb3d_clip_duration",
  "mb3d_anim",
  "mb3d_node_morph",
  "mb3d_node_material",
  "mb3d_node_fade",
  "mb3d_draw_distance",
  "mb3d_texture_filter",
  "mb3d_node_get_parent",
  "mb3d_ripple",
  "mb3d_wake"
];
var MATERIAL_SIZE = 64;
var SUN_SIZE = 36;
var SKY_SIZE = 64;
var POST_SIZE = 32;
var EMITTER_SIZE = 128;
var TRAIL_SIZE = 64;
var INSTANCE_SIZE = 44;
var SKIN_JOINT_SIZE = 68;
var MAX_JOINTS = 64;
var MAX_INSTANCES_PER_NODE = 16384;
var FOG_SIZE = 48;
var STYLE_SIZE = 32;
var TEXT_DESC_SIZE = 64;
var DOF_SIZE = 16;
var SPEED_SIZE = 32;
var REFLECTOR_SIZE = 48;
var ATMOSPHERE_SIZE = 80;
var CLOUDS_SIZE = 64;
var SKY_DISC_SIZE = 64;
var SKY_TEXTURE_SIZE = 32;
var WATER_SIZE = 96;
var GRADE_SIZE = 96;
var SHAFTS_SIZE = 48;
var HALOS_SIZE = 32;
var FILM_SIZE = 32;
var RETRO_SIZE = 32;
var LAYER_SIZE = 40;
var MAX_LAYERS = 4;
var MAX_MASK = 256;
var NAME_MAX = 256;
var TEXT_MAX_BYTES = 4096;
var MESH_MAX_BYTES = 64 * 1024 * 1024;
var STORE_QUOTA = 256 * 1024;
var STORE_KEY_MAX = 64;
var PROGRESS_PREFIX = "progress/";
var PROGRESS_QUOTA = 64 * 1024;
function declared(req, m) {
  if (!req) return true;
  if (req.stdlib) return (m.stdlib[req.stdlib] ?? 0) >= (req.version ?? 1);
  if (req.sensor) return m.sensors.includes(req.sensor);
  if (req.capability) return m.capabilities.includes(req.capability);
  return false;
}
function base64(b) {
  let s = "";
  for (const c of b) s += String.fromCharCode(c);
  return btoa(s);
}
function buildImports(s, host, m) {
  const utf8 = new TextDecoder("utf-8", { fatal: false });
  const enc = new TextEncoder();
  const bytes = () => new Uint8Array(s.memory().buffer);
  const view = () => new DataView(s.memory().buffer);
  const slice = (ptr, len) => bytes().subarray(ptr, ptr + len);
  const str = (ptr, len) => utf8.decode(slice(ptr, len));
  const toHost = (data) => {
    const at = host.host_scratch(data.length);
    new Uint8Array(host.memory.buffer, at, data.length).set(data);
  };
  const boards = new Set((m.scores ?? []).map((b) => b.board));
  const progress = m.capabilities.includes("progress");
  let f32 = new Float32Array(0);
  const floats = (ptr, n) => {
    const buf = s.memory().buffer;
    if (ptr & 3) {
      const v = new DataView(buf);
      return Array.from({ length: n }, (_, i) => v.getFloat32(ptr + i * 4, true));
    }
    if (f32.buffer !== buf) f32 = new Float32Array(buf);
    const at = ptr >>> 2;
    return Array.from(f32.subarray(at, at + n));
  };
  const assetToHost = (asset) => {
    const st = s.assets.state(asset);
    if (st === 0) return -6 /* NotReady */;
    const data = s.assets.data(asset);
    if (st < 0 || !data) return -1 /* InvalidHandle */;
    toHost(data);
    return data.length;
  };
  const refusedOnce = /* @__PURE__ */ new Set();
  const refused = (asset, r) => {
    if (r !== -4 /* Quota */) return r;
    const why = host_take_refusal();
    if (why && !refusedOnce.has(asset)) {
      refusedOnce.add(asset);
      const msg = `runtime: ${s.assets.path(asset) ?? `asset ${asset}`} refused (\u22124): ${why}`;
      console.warn(msg);
      post({ op: "log", level: 2, msg });
    }
    return r;
  };
  const impl = {
    // 5.1 sys
    mb_log: ((level, ptr, len) => s.log(level, str(ptr, len))),
    mb_mark: ((ptr, len) => s.mark(str(ptr, Math.min(len >>> 0, 64)))),
    mb_param: ((nptr, nlen, out, cap) => {
      const v = s.params.get(str(nptr, nlen));
      if (v === void 0) return -1;
      const b = enc.encode(v);
      bytes().set(b.subarray(0, Math.max(0, cap)), out);
      return b.length;
    }),
    mb_time: (() => s.gameTime),
    mb_time_lost: (() => s.timeLost),
    mb_rand_seed: (() => BigInt.asIntN(64, s.seed)),
    mb_daily_seed: (() => BigInt.asIntN(64, s.dailySeed)),
    mb_round: ((state) => {
      if (state < 0 || state > 2 || state === s.round) return;
      s.round = state;
      s.observer?.round(state);
      if (s.replaying || s.fastForwarding) return;
      if (state === 1) s.play?.start();
      const play = state === 2 ? s.play?.finish() ?? null : null;
      post(play ? { op: "round", state, play } : { op: "round", state });
    }),
    mb_player_id: ((ptr) => {
      bytes().set(s.playerId, ptr);
      return s.playerId.length;
    }),
    mb_locale: ((ptr, cap) => {
      const b = enc.encode(s.locale);
      bytes().set(b.subarray(0, cap), ptr);
      return b.length;
    }),
    mb_screen: ((ptr) => {
      const v = view();
      s.screen.forEach((x, i) => v.setFloat32(ptr + i * 4, x, true));
    }),
    mb_asset_load: ((ptr, len) => s.assets.load(str(ptr, len))),
    mb_asset_state: ((h) => s.assets.state(h)),
    mb_asset_read: ((h, ptr, cap) => {
      const st = s.assets.state(h);
      if (st === 0) return -6 /* NotReady */;
      if (st < 0) return st;
      const data = s.assets.data(h);
      if (!data) return -1 /* InvalidHandle */;
      bytes().set(data.subarray(0, Math.max(0, cap)), ptr);
      return data.length;
    }),
    // 5.2 input
    mb_input_poll: ((ptr, cap) => s.pollInput(view(), ptr, cap)),
    // 5.3 mb2d, memory-passing calls
    mb2d_poly: ((ptr, n, rgba) => {
      if (n < 3 || n > 512) return;
      toHost(slice(ptr, n * 8));
      host.mbh_poly(n, rgba);
    }),
    mb2d_text: ((font, size, x, y, rgba, ptr, len) => {
      toHost(slice(ptr, Math.min(len, 4096)));
      host.mbh_text(font, size, x, y, rgba, Math.min(len, 4096));
    }),
    mb2d_measure: ((font, size, ptr, len) => {
      toHost(slice(ptr, Math.min(len, 4096)));
      return host.mbh_measure(font, size, Math.min(len, 4096));
    }),
    mb2d_image: ((asset) => {
      const st = s.assets.state(asset);
      if (st === 0) return -6 /* NotReady */;
      const data = s.assets.data(asset);
      if (st < 0 || !data) return -1 /* InvalidHandle */;
      toHost(data);
      return host.mbh_image(data.length);
    }),
    // mb2d 1's never-implemented font call; mb2d 2 games use mb2d_font_load (SPEC §5.3).
    mb2d_font: (() => -7 /* Unsupported */),
    // 5.3 mb2d 2: game fonts (.mbf baked by `mb font add`).
    mb2d_font_load: ((asset) => {
      const n = assetToHost(asset);
      return n < 0 ? n : host.mbh_font_load(n);
    }),
    mb2d_font_metrics: ((font, out) => {
      const r = host.mbh_font_metrics(font);
      if (r >= 0) bytes().set(new Uint8Array(host.memory.buffer, host.host_scratch(32), 32), out);
      return r;
    }),
    // 5.4 mb3d, memory-passing calls
    mb3d_mesh: ((ptr, len) => {
      if (len < 12 || len > MESH_MAX_BYTES) return -2 /* InvalidArgument */;
      toHost(slice(ptr, len));
      return host.mbh_mesh(len);
    }),
    mb3d_texture: ((asset) => {
      const n = assetToHost(asset);
      return n < 0 ? n : refused(asset, host.mbh_texture(n));
    }),
    mb3d_gltf: ((asset) => {
      const n = assetToHost(asset);
      return n < 0 ? n : refused(asset, host.mbh_gltf(n));
    }),
    mb3d_material: ((ptr) => {
      toHost(slice(ptr, MATERIAL_SIZE));
      return host.mbh_material();
    }),
    mb3d_material_set: ((handle, ptr) => {
      toHost(slice(ptr, MATERIAL_SIZE));
      return host.mbh_material_set(handle);
    }),
    mb3d_node_transform: ((node, ptr) => host.mbh_node_transform(node, ...floats(ptr, 10))),
    mb3d_camera: ((ptr) => host.mbh_camera(...floats(ptr, 10))),
    mb3d_project: ((x, y, z, out) => {
      const r = host.mbh_project(x, y, z);
      const at = host.host_scratch(12);
      bytes().set(new Uint8Array(host.memory.buffer, at, 12), out);
      return r;
    }),
    mb3d_sun: ((ptr) => {
      toHost(slice(ptr, SUN_SIZE));
      host.mbh_sun();
    }),
    mb3d_sky: ((ptr) => {
      toHost(slice(ptr, SKY_SIZE));
      host.mbh_sky();
    }),
    mb3d_post: ((ptr) => {
      toHost(slice(ptr, POST_SIZE));
      host.mbh_post();
    }),
    mb3d_emitter: ((ptr) => {
      toHost(slice(ptr, EMITTER_SIZE));
      return host.mbh_emitter();
    }),
    mb3d_trail: ((ptr) => {
      toHost(slice(ptr, TRAIL_SIZE));
      return host.mbh_trail();
    }),
    // 5.4 mb3d 2, memory-passing calls
    mb3d_instances: ((node, ptr, count) => {
      if (count < 0 || count > MAX_INSTANCES_PER_NODE) return -4 /* Quota */;
      toHost(slice(ptr, count * INSTANCE_SIZE));
      return host.mbh_instances(node, count);
    }),
    mb3d_skin: ((node, ptr, count) => {
      if (count < 0 || count > MAX_JOINTS) return -4 /* Quota */;
      toHost(slice(ptr, count * SKIN_JOINT_SIZE));
      return host.mbh_skin(node, count);
    }),
    mb3d_clip_find: ((model, ptr, len) => {
      if (len < 0 || len > NAME_MAX) return -2 /* InvalidArgument */;
      toHost(slice(ptr, len));
      return host.mbh_clip_find(model, len);
    }),
    mb3d_node_find: ((node, ptr, len) => {
      if (len < 0 || len > NAME_MAX) return -2 /* InvalidArgument */;
      toHost(slice(ptr, len));
      return host.mbh_node_find(node, len);
    }),
    mb3d_node_world: ((node, out) => {
      const r = host.mbh_node_world(node);
      if (r >= 0) bytes().set(new Uint8Array(host.memory.buffer, host.host_scratch(64), 64), out);
      return r;
    }),
    mb3d_material_get: ((material, out) => {
      const r = host.mbh_material_get(material);
      if (r >= 0) bytes().set(new Uint8Array(host.memory.buffer, host.host_scratch(MATERIAL_SIZE), MATERIAL_SIZE), out);
      return r;
    }),
    mb3d_model_materials: ((model, out, cap) => {
      const r = host.mbh_model_materials(model, Math.max(0, Math.min(cap, 1024)));
      const n = Math.min(Math.max(r, 0), Math.max(0, Math.min(cap, 1024))) * 4;
      if (n > 0) bytes().set(new Uint8Array(host.memory.buffer, host.host_scratch(n), n), out);
      return r;
    }),
    mb3d_model_node_names: ((model, out, cap) => {
      const c = Math.max(0, Math.min(cap, 65536));
      const r = host.mbh_model_node_names(model, c);
      const n = Math.min(Math.max(r, 0), c);
      if (n > 0) bytes().set(new Uint8Array(host.memory.buffer, host.host_scratch(n), n), out);
      return r;
    }),
    mb3d_fog: ((ptr) => {
      toHost(slice(ptr, FOG_SIZE));
      host.mbh_fog();
    }),
    mb3d_material_style: ((material, ptr) => {
      toHost(slice(ptr, STYLE_SIZE));
      return host.mbh_material_style(material);
    }),
    mb3d_text: ((node, desc, ptr, len) => {
      if (len < 0 || len > TEXT_MAX_BYTES) return -2 /* InvalidArgument */;
      const b = new Uint8Array(TEXT_DESC_SIZE + len);
      b.set(slice(desc, TEXT_DESC_SIZE), 0);
      b.set(slice(ptr, len), TEXT_DESC_SIZE);
      toHost(b);
      return host.mbh_text3d(node, len);
    }),
    mb3d_dof: ((ptr) => {
      toHost(slice(ptr, DOF_SIZE));
      host.mbh_dof();
    }),
    mb3d_speed: ((ptr) => {
      toHost(slice(ptr, SPEED_SIZE));
      host.mbh_speed();
    }),
    mb3d_reflector: ((ptr) => {
      toHost(slice(ptr, REFLECTOR_SIZE));
      host.mbh_reflector();
    }),
    mb3d_atmosphere: ((ptr) => {
      toHost(slice(ptr, ATMOSPHERE_SIZE));
      host.mbh_atmosphere();
    }),
    mb3d_clouds: ((ptr) => {
      toHost(slice(ptr, CLOUDS_SIZE));
      host.mbh_clouds();
    }),
    mb3d_sky_disc: ((ptr) => {
      toHost(slice(ptr, SKY_DISC_SIZE));
      host.mbh_sky_disc();
    }),
    mb3d_sky_texture: ((texture, ptr) => {
      toHost(slice(ptr, SKY_TEXTURE_SIZE));
      return host.mbh_sky_texture(texture);
    }),
    mb3d_water: ((ptr) => {
      toHost(slice(ptr, WATER_SIZE));
      host.mbh_water();
    }),
    mb3d_grade: ((ptr) => {
      toHost(slice(ptr, GRADE_SIZE));
      host.mbh_grade();
    }),
    mb3d_shafts: ((ptr) => {
      toHost(slice(ptr, SHAFTS_SIZE));
      host.mbh_shafts();
    }),
    mb3d_halos: ((ptr) => {
      toHost(slice(ptr, HALOS_SIZE));
      host.mbh_halos();
    }),
    mb3d_film: ((ptr) => {
      toHost(slice(ptr, FILM_SIZE));
      host.mbh_film();
    }),
    mb3d_retro: ((ptr) => {
      toHost(slice(ptr, RETRO_SIZE));
      host.mbh_retro();
    }),
    mb3d_backdrop: ((ptr, count) => {
      if (count < 0 || count > MAX_LAYERS) return -2 /* InvalidArgument */;
      toHost(slice(ptr, count * LAYER_SIZE));
      return host.mbh_backdrop(count);
    }),
    mb3d_anim_mask: ((node, clip, time, weight, ptr, count) => {
      if (count < 0 || count > MAX_MASK) return -2 /* InvalidArgument */;
      toHost(slice(ptr, count * 8));
      return host.mbh_anim_mask(node, clip, time, weight, count);
    }),
    mb3d_anim_take: ((node, out) => {
      const r = host.mbh_anim_take(node);
      if (r >= 0) bytes().set(new Uint8Array(host.memory.buffer, host.host_scratch(40), 40), out);
      return r;
    }),
    // 5.6 audio
    mb_sound: ((asset) => {
      const st = s.assets.state(asset);
      if (st === 0) return -6 /* NotReady */;
      const data = s.assets.data(asset);
      if (st < 0 || !data) return -1 /* InvalidHandle */;
      return s.audio.load(data);
    }),
    mb_play: ((sound, vol, pan, pitch, loop) => s.audio.play(sound, vol, pan, pitch, loop !== 0)),
    mb_play_at: ((sound, vol, pan, pitch, loop, at) => s.audio.play(sound, vol, pan, pitch, loop !== 0, Number.isFinite(at) ? at : s.gameTime)),
    mb_play_from: ((sound, vol, pan, pitch, loop, at, from, fade) => s.audio.play(sound, vol, pan, pitch, loop !== 0, Number.isFinite(at) ? at : s.gameTime, from, fade)),
    mb_voice_set: ((voice, vol, pan, pitch) => s.audio.update(voice, vol, pan, pitch)),
    mb_voice_stop: ((voice) => s.audio.stop(voice)),
    mb_voice_stop_at: ((voice, at, fade) => s.audio.stopAt(voice, Number.isFinite(at) ? at : s.gameTime, fade)),
    mb_voice_bus: ((voice, bus) => s.audio.voiceBus(voice, bus)),
    mb_bus_fx: ((bus, kind, ptr) => s.audio.busFx(bus, kind, floats(ptr, FX_FIELDS))),
    // 5.7 sensors
    mb_tilt: ((ptr) => {
      if (!s.tilt) return -6 /* NotReady */;
      const v = view();
      s.tilt.forEach((x, i) => v.setFloat32(ptr + i * 4, x, true));
      return 0;
    }),
    mb_motion: ((ptr) => {
      if (!s.motion) return -6 /* NotReady */;
      const v = view();
      s.motion.forEach((x, i) => v.setFloat32(ptr + i * 4, x, true));
      return 0;
    }),
    // Attitude (28 bytes): quaternion x, y, z, w then rotation rate x, y, z.
    mb_attitude: ((ptr) => {
      if (!s.attitude) return -6 /* NotReady */;
      const v = view();
      s.attitude.forEach((x, i) => v.setFloat32(ptr + i * 4, x, true));
      return 0;
    }),
    // −1 until a reading arrives or when the mic is unavailable (SPEC §5.7).
    mb_loudness: (() => s.loudness ?? -1),
    // Output only, so it never affects determinism; the native side decides whether to play it.
    mb_haptic: ((kind) => {
      if (kind >= 0 && kind <= 3 && !s.replaying && !s.fastForwarding) {
        post({ op: "haptic", kind });
        s.onHaptic?.(kind);
      }
    }),
    // 5.8 store
    mb_store_get: ((kp, kl, bp, cap) => {
      const v = s.store.get(str(kp, kl));
      if (!v) return -1;
      bytes().set(v.subarray(0, Math.max(0, cap)), bp);
      return v.length;
    }),
    mb_store_set: ((kp, kl, vp, vl) => {
      if (kl === 0 || kl > STORE_KEY_MAX) return -2 /* InvalidArgument */;
      const key = str(kp, kl);
      const value = slice(vp, vl).slice();
      const old = s.store.get(key);
      const size = s.storeBytes() - (old ? key.length + old.length : 0) + (vl ? key.length + vl : 0);
      if (size > STORE_QUOTA) return -4 /* Quota */;
      if (progress && key.startsWith(PROGRESS_PREFIX)) {
        const p = s.storeBytes(PROGRESS_PREFIX) - (old ? key.length + old.length : 0) + (vl ? key.length + vl : 0);
        if (p > PROGRESS_QUOTA) return -4 /* Quota */;
      }
      if (vl === 0) s.store.delete(key);
      else s.store.set(key, value);
      if (!s.replaying) {
        const b64 = vl ? base64(value) : null;
        post({ op: "store_set", key, value: b64 });
        s.onStore?.(key, b64);
      }
      return 0;
    }),
    // 5.8 score
    mb_score_submit: ((board, value) => {
      if (!boards.has(board)) return -3 /* NotDeclared */;
      s.observer?.score(board, value);
      if (!s.replaying && !s.fastForwarding) post({ op: "score_submit", board, value: value.toString() });
      return 0;
    }),
    mb_score_show: ((board) => {
      if (boards.has(board) && !s.replaying && !s.fastForwarding) post({ op: "score_show", board });
    }),
    // 5.9 links: always through the platform's interstitial.
    mb_link_open: ((ptr, len) => {
      if (len > 2048) return -2 /* InvalidArgument */;
      const url = str(ptr, len);
      if (!/^https?:\/\/[^\s]+$/i.test(url)) return -2 /* InvalidArgument */;
      if (!s.replaying && !s.fastForwarding) post({ op: "link_open", url });
      return 0;
    })
  };
  for (const name of HOST_DIRECT) {
    const f = host[name];
    if (typeof f === "function") impl[name] = f;
  }
  const imports = {};
  for (const entry of abi_default.imports) {
    const { name } = entry;
    const req = entry.requires;
    if (!declared(req, m)) {
      imports[name] = (() => {
        throw new Error(`mb.${name} used without declaring ${JSON.stringify(req)} in manifest.toml`);
      });
    } else {
      imports[name] = impl[name] ?? (() => {
        throw new Error(`mb.${name} is not implemented by this host yet`);
      });
    }
  }
  return imports;
}

// src/session.ts
var NOTES_PER_FRAME = 8;
var LEVELS = ["debug", "info", "warn", "error"];
var Session = class {
  constructor(audio, seed, dailySeed, playerId, locale, screen2, gameBase, store, assetCache) {
    this.audio = audio;
    this.seed = seed;
    this.dailySeed = dailySeed;
    this.playerId = playerId;
    this.locale = locale;
    this.screen = screen2;
    this.assets = new Assets(gameBase, assetCache);
    for (const [k, v] of Object.entries(store)) this.store.set(k, Uint8Array.from(atob(v), (c) => c.charCodeAt(0)));
  }
  gameTime = 0;
  /** Wall-clock seconds the current frame dropped (SPEC §5.1 mb_time_lost). */
  timeLost = 0;
  frameEvents = [];
  /** Game time of each of this frame's events. */
  frameTimes = [];
  cursor = 0;
  memory;
  assets;
  /** Saved data: in-page copy, written through to native (SPEC §5.8). */
  store = /* @__PURE__ */ new Map();
  /** Replays never persist or submit anything. */
  replaying = false;
  /** Running through a left run's frames to pick it up (`mb.restart({ resume })`): what it
   *  saves and reports still counts, but nothing reaches the player (haptics, overlays). */
  fastForwarding = false;
  /** Called when saved data changes (key, base64 value or null). */
  onStore = null;
  /** What the game reports, replay or not: the headless runner's (`mb replay`)
   *  view of a run. Nothing here reaches native. */
  observer = null;
  /** Last SPEC §5.1 round state the game reported (0 idle, 1 playing, 2 over). */
  round = 0;
  /** Live sessions on a page: how each round is played, reported with its end. */
  play = null;
  /** A haptic the game played (web: rumbles the controller in use). */
  onHaptic = null;
  /** This frame's sensor snapshot (SPEC §5.7); null before the first sample. */
  tilt = null;
  motion = null;
  /** Quaternion xyzw and rotation rate xyz (rad/s). */
  attitude = null;
  loudness = null;
  /** What the game marked (`mb_mark`) and logged (`mb_log`) since its frame
   *  began, for the tools (`mb.step()` timings, `mb.stats()` frames): never
   *  hashed, recorded or visible to the guest. */
  marks = [];
  logs = [];
  begun = false;
  /** Debug parameters (SPEC §5.1 `mb_param`): the preview tools' `--param`,
   *  fixed for the session and recorded in its log; always empty on a device. */
  params = /* @__PURE__ */ new Map();
  /** Total bytes of keys + values (those starting with `prefix`), for the quotas. */
  storeBytes(prefix = "") {
    let n = 0;
    for (const [k, v] of this.store) if (k.startsWith(prefix)) n += k.length + v.length;
    return n;
  }
  /** Starts a frame: delivers its events and sensor snapshot, advances game
   *  time by dt. An event's time is the frame's game time less its recorded
   *  lag, never before the previous frame's time and never before the event
   *  ahead of it (SPEC §5.2); computed from recorded values only, so a replay
   *  sees exactly the same times. */
  beginFrame(dt, events, sensors = {}, lost = 0) {
    const prev = this.gameTime;
    this.gameTime += dt;
    if (this.begun) {
      this.marks = [];
      this.logs = [];
    }
    this.begun = true;
    this.timeLost = lost;
    if (sensors.tilt !== void 0) this.tilt = sensors.tilt;
    if (sensors.motion !== void 0) this.motion = sensors.motion;
    if (sensors.att !== void 0) this.attitude = sensors.att;
    if (sensors.loud !== void 0) this.loudness = sensors.loud;
    if (sensors.screen?.length === this.screen.length) this.screen.set(sensors.screen);
    this.frameEvents = events;
    let last = prev;
    this.frameTimes = events.map((ev) => {
      const lag = ev[8];
      const t = lag ? Math.min(this.gameTime, Math.max(last, this.gameTime - lag / 1e6)) : this.gameTime;
      last = t;
      return t;
    });
    this.cursor = 0;
  }
  pollInput(view, ptr, cap) {
    const n = Math.max(0, Math.min(cap, this.frameEvents.length - this.cursor));
    const end = this.cursor + n;
    writeEvents(view, ptr, this.frameEvents.slice(this.cursor, end), this.frameTimes.slice(this.cursor, end));
    this.cursor += n;
    return n;
  }
  log(level, msg) {
    const fn = level >= 3 ? console.error : level === 2 ? console.warn : console.log;
    fn(`[game] ${msg}`);
    post({ op: "log", level, msg: msg.slice(0, 2e3) });
    if (this.logs.length < NOTES_PER_FRAME) this.logs.push(`${LEVELS[level] ?? "info"}: ${msg.slice(0, 200)}`);
  }
  /** SPEC §5.1 `mb_mark`: a no-op for the game; the tools see it. */
  mark(label) {
    if (this.marks.length < NOTES_PER_FRAME) this.marks.push(label);
  }
};
var Assets = class {
  constructor(base, cache) {
    this.base = base;
    this.cache = cache;
  }
  list = [];
  load(path) {
    if (!/^[A-Za-z0-9_./-]+$/.test(path) || path.split("/").some((c) => c === "" || c === "." || c === "..")) {
      return -2;
    }
    const a = { path, state: 0, fetching: Promise.resolve() };
    let bytes = this.cache.get(path);
    if (!bytes) {
      bytes = fetch(`${this.base}${path}`).then(async (r) => r.ok ? { state: 1, data: new Uint8Array(await r.arrayBuffer()) } : { state: -1 }).catch(() => ({ state: -1 }));
      this.cache.set(path, bytes);
    }
    a.fetching = bytes.then((r) => {
      a.arrived = r;
    });
    this.list.push(a);
    return this.list.length;
  }
  get(h) {
    return this.list[h - 1];
  }
  state(h) {
    return this.get(h)?.state ?? -1;
  }
  /** The bundle path an asset handle was loaded from. */
  path(h) {
    return this.get(h)?.path;
  }
  data(h) {
    return this.get(h)?.data;
  }
  /** Every asset requested so far has arrived (fetched or failed). */
  allArrived() {
    return this.list.every((a) => a.state !== 0 || a.arrived !== void 0);
  }
  /** Live play: publishes everything that has arrived; returns the handles. */
  publishArrived() {
    const out = [];
    this.list.forEach((a, i) => {
      if (a.state === 0 && a.arrived) {
        this.publish(a);
        out.push(i + 1);
      }
    });
    return out;
  }
  /** Real-time replay: publishes the recorded handles if they've all arrived. */
  tryPublishRecorded(handles) {
    const assets = handles.map((h) => this.get(h));
    if (assets.some((a) => !a || a.state === 0 && !a.arrived)) return false;
    for (const a of assets) if (a && a.state === 0) this.publish(a);
    return true;
  }
  /** Replay: publishes exactly the recorded handles, waiting for their fetches. */
  async publishRecorded(handles) {
    for (const h of handles) {
      const a = this.get(h);
      if (!a) throw new Error(`replay references unknown asset handle ${h}`);
      await a.fetching;
      this.publish(a);
    }
  }
  /** Watch-mode restore and the headless runner: publishes the recorded
   *  handles that exist and are still pending (other code may load other
   *  assets), waiting for their fetches; returns the ones published. */
  async publishKnown(handles) {
    const out = [];
    for (const h of handles) {
      const a = this.get(h);
      if (!a || a.state !== 0) continue;
      await a.fetching;
      this.publish(a);
      out.push(h);
    }
    return out;
  }
  publish(a) {
    a.state = a.arrived?.state ?? -1;
    a.data = a.arrived?.data;
  }
};

// src/run.ts
var INIT_BUDGET_MS = 500;
var CALLBACK_BUDGET_MS = 250;
var HEARTBEAT_EVERY = 15;
var FUEL_PER_CALLBACK = 3e8;
var PREROLL_MIN_FRAMES = 6;
var PREROLL_MAX_MS = 3e3;
var GPU_WAIT_MAX_MS = 1e3;
var MAX_DT = 0.1;
var RESIZE = 14;
var TOP_OVER = 8;
var MAX_FRAME_HASHES = 216e3;
var Runtime = class _Runtime {
  constructor(guest, session, input, recorder, gamepads, sensors, stats, latency, source, view = null, play = null) {
    this.guest = guest;
    this.session = session;
    this.input = input;
    this.recorder = recorder;
    this.gamepads = gamepads;
    this.sensors = sensors;
    this.stats = stats;
    this.latency = latency;
    this.source = source;
    this.view = view;
    this.play = play;
  }
  running = false;
  suspended = false;
  /** mb_suspend was called and mb_resume is owed (live sessions only). */
  guestSuspended = false;
  last = null;
  /** Previous displayed frame's timestamp, for mb3d's adaptive render scale. */
  lastShown = null;
  /** Frames this session has run: one per mb_update. */
  frames = 0;
  /** Display ticks, for the heartbeat (a replay waiting on an asset ticks without running a frame). */
  ticks = 0;
  /** Draw hash after mb_init (index 0) and after each frame. */
  hashes = [];
  stopped = false;
  raf = 0;
  /** Next frame of a replay being played back. */
  cursor = 0;
  /** Lifecycle callbacks made since the last recorded frame. */
  calls = [];
  /** Serializes `stepFrames` (a replay step may wait for an asset). */
  stepping = Promise.resolve();
  /** The rAF timestamp of the frame being run (null outside a display frame). */
  frameNow = null;
  /** rAF to rAF (ms) for the frame being run, for `mb.stats()`; null outside a display frame. */
  frameInterval = null;
  /** Sensor readings last handed to the guest (recorded only when they change). */
  seen = { tilt: "", motion: "", att: "", loud: null };
  onReplayEnd = null;
  /** Preview only (SPEC §9): multiplies live dt, for watching fast moments slowly. */
  static timeScale = 1;
  /** The SPEC §3 wall-clock budgets apply (a phone, a browser). The headless
   *  runner turns them off: how fast a server is mustn't change how a replay
   *  goes. Loop fuel still applies; it counts iterations, not time. */
  static budgets = true;
  /** CPU ms of the last 120 calls of each callback and of presenting (`mb.stats()`). */
  static timings = /* @__PURE__ */ new Map();
  static cpu(name, ms) {
    let t = _Runtime.timings.get(name);
    if (!t) _Runtime.timings.set(name, t = []);
    t.push(ms);
    if (t.length > 120) t.shift();
  }
  /** Runs mb_init; the draw hash at frame 0 covers what it did. */
  init() {
    this.timed("mb_init", () => this.guest.mb_init(), INIT_BUDGET_MS);
    this.hashes.push(host_draw_hash());
  }
  start() {
    this.running = true;
    this.session.audio.resume();
    this.raf = requestAnimationFrame((t) => this.tick(t));
  }
  /** Preview (SPEC §9): no frames at all until `mb.step(n)` or `mb.resume()`,
   *  so a live run and its replay can be stepped side by side from frame 0. */
  startHeld() {
    this.running = true;
    this.suspended = true;
    this.session.audio.suspend();
  }
  /** Feed neighbour: run silently until the assets it asked for have loaded
   *  (so the waiting card shows the real game, not its loading fallback),
   *  then hold that frame until `resume()`. */
  startPaused() {
    this.running = true;
    this.suspended = true;
    this.prerolling = true;
    this.session.audio.setMuted(true);
    const deadline = performance.now() + PREROLL_MAX_MS;
    let n = 0;
    const frame = () => {
      if (!this.prerolling || this.stopped) return;
      this.presenting = false;
      if (this.source.kind === "replay") this.replayTick();
      else this.liveFrame(Math.fround(1 / 60));
      this.presenting = true;
      n++;
      const loaded = this.session.assets.allArrived() && n >= PREROLL_MIN_FRAMES;
      if (loaded || performance.now() > deadline) {
        this.prerolling = false;
        this.session.audio.setMuted(false);
        if (!this.stopped) present(host_end_frame);
        return;
      }
      this.raf = requestAnimationFrame(frame);
    };
    frame();
  }
  /** When the last resume happened (ms), until the first controller input after it is
   *  delivered: logged, to see how soon a resumed game hears the controller. */
  resumedAt = null;
  /** Live frames run since that resume. */
  framesSinceResume = 0;
  /** Input turned on for play (`mb.inputs(true)`): time the first controller input from
   *  there, not from the card's resume, which came while it was browsed. */
  inputsOn() {
    this.resumedAt = performance.now();
    this.framesSinceResume = 0;
  }
  /** Running silently ahead of being shown (startPaused). */
  prerolling = false;
  /** False while prerolling: frames render without being presented. */
  presenting = true;
  /** `quiet`: a pause the player will likely come straight back from (the
   *  current card): audio is silenced rather than stopped (Audio.suspend). */
  suspend(quiet = false) {
    if (this.stopped) return;
    if (this.suspended) {
      if (!quiet) this.session.audio.suspend(false);
      return;
    }
    this.suspended = true;
    cancelFrame(this.raf);
    this.session.audio.suspend(quiet);
    if (this.source.kind === "live") {
      this.lifecycle("suspend");
      this.calls.push("suspend");
      this.guestSuspended = true;
    }
    post({ op: "suspended" });
  }
  resume() {
    if (!this.suspended || this.stopped) return;
    if (this.prerolling) {
      this.prerolling = false;
      cancelFrame(this.raf);
      this.session.audio.setMuted(false);
    }
    this.suspended = false;
    this.last = null;
    this.lastShown = null;
    this.input.take();
    this.session.audio.resume();
    if (this.guestSuspended) {
      this.guestSuspended = false;
      this.lifecycle("resume");
      this.calls.push("resume");
    }
    post({ op: "resumed" });
    this.resumedAt = performance.now();
    this.framesSinceResume = 0;
    this.raf = requestAnimationFrame((t) => this.tick(t));
  }
  lifecycle(c) {
    if (c === "suspend") this.timed("mb_suspend", () => this.guest.mb_suspend?.());
    else this.timed("mb_resume", () => this.guest.mb_resume?.());
  }
  tick(now) {
    if (!this.running || this.suspended || this.stopped) return;
    this.frameInterval = this.lastShown !== null ? now - this.lastShown : null;
    if (this.frameInterval !== null) host_frame_time(this.frameInterval);
    this.lastShown = now;
    this.frameNow = now;
    if (this.source.kind === "replay") {
      this.replayTick();
    } else {
      const raw = this.last === null ? 1 / 60 : Math.max((now - this.last) / 1e3, 0);
      this.last = now;
      const scaled = raw * _Runtime.timeScale;
      const dt = Math.fround(Math.min(scaled, MAX_DT));
      const lost = scaled > MAX_DT ? Math.fround(scaled - dt) : 0;
      this.liveFrame(dt, { now, dt, scale: _Runtime.timeScale }, lost);
    }
    this.frameNow = null;
    this.frameInterval = null;
    if (++this.ticks % HEARTBEAT_EVERY === 0) {
      post({ op: "heartbeat", frame: this.ticks, time: this.session.gameTime });
    }
    if (!this.stopped) this.raf = requestAnimationFrame((t) => this.tick(t));
  }
  /** A live frame. With `clock` (a real display frame), events get their own
   *  times within it; without (prerolling, stepping), the frame's time. */
  liveFrame(dt, clock, lost = 0) {
    for (const e of this.view?.pending?.splice(0) ?? []) this.input.push(e);
    this.gamepads?.poll((e) => this.input.push(e));
    const events = this.input.take(clock);
    const assets = this.session.assets.publishArrived();
    const sensors = this.sampleSensors();
    const screen2 = this.view?.adaptive ? this.view.screen : null;
    if (screen2 && screen2.some((v, i) => v !== this.session.screen[i])) {
      sensors.screen = Array.from(screen2);
      events.unshift([RESIZE, 0, 0, screen2[0], screen2[1], 0, 0, 0]);
    }
    this.play?.frame(dt);
    if (this.resumedAt !== null) {
      this.framesSinceResume++;
      if (events.some((e) => e[0] === 11 || e[0] === 12)) {
        post({ op: "log", level: 0, msg: `runtime: first controller input ${Math.round(performance.now() - this.resumedAt)} ms (${this.framesSinceResume} frames) after resume or input on` });
        this.resumedAt = null;
      }
    }
    this.latency.deliver(this.input.taken, this.frames + 1, this.frameNow);
    this.recorder.frame(dt, events, assets, sensors, this.calls.splice(0), lost);
    return this.step(dt, events, sensors, [], lost);
  }
  /** Preview: advance a suspended session by `n` frames — a live one by
   *  `dt` seconds each (1/60 s unless given; recorded like any frame), a
   *  replay by its next `n` recorded frames (waiting for assets as needed;
   *  it stops at the end rather than looping). Resolves once they have run,
   *  with what each cost.
   *
   *  `paced` (the default) waits for the GPU to finish each frame before
   *  running the next, as the display does: submitting frames back to back
   *  backs up the GPU queue until a submit blocks, which reads as a hitch
   *  the game doesn't have.
   *
   *  `until` stops early, after the first frame whose marks (`mb_mark`)
   *  include one of these labels (`mb shot --at mark:<label>`). */
  stepFrames(n, paced = true, dt = 1 / 60, until) {
    if (!(dt > 0 && dt <= MAX_DT)) return Promise.reject(new Error(`mb.step: dt ${dt} is not seconds in (0, ${MAX_DT}]`));
    const frameDt = Math.fround(dt);
    const run = async () => {
      const timings = [];
      if (paced) await gpuDone();
      for (let i = 0; i < n && this.suspended && !this.stopped; i++) {
        let stat;
        if (this.source.kind === "live") {
          stat = this.liveFrame(frameDt);
        } else {
          const f = this.source.log.frames[this.cursor];
          if (!f) break;
          await this.session.assets.publishRecorded(f.assets ?? []);
          this.cursor++;
          stat = this.step(f.dt, f.events ?? [], f, f.calls, f.lost);
        }
        if (!stat) break;
        if (paced && stat.present !== null) {
          const submitted = performance.now();
          await gpuDone();
          stat.gpu = performance.now() - submitted;
        }
        const t = { frame: stat.frame, mb_update: stat.mb_update, mb_render: stat.mb_render, present: stat.present, gpu: stat.gpu };
        if (stat.camera) t.camera = stat.camera;
        if (stat.track) t.track = stat.track;
        if (stat.marks) t.marks = stat.marks;
        if (stat.logs) t.logs = stat.logs;
        timings.push(t);
        if (until?.length && stat.marks?.some((m) => until.includes(m))) break;
      }
      return { ...this.brief(), timings };
    };
    const p = this.stepping.then(run);
    this.stepping = p.catch(() => {
    });
    return p;
  }
  /** Preview (SPEC §9 `mb.gpu`): draws the frame on screen again `n` times,
   *  each after the GPU has finished everything before it, and resolves to
   *  each one's GPU ms (submit until done), so one scene can be timed as
   *  often as it takes to trust the number. Presentation only: no guest
   *  call, nothing hashed, recorded or simulated. */
  redrawTimed(n) {
    if (!this.held) return Promise.reject(new Error("mb.gpu: pause the session first (mb.suspend(), or restart with hold)"));
    const run = async () => {
      const ms = [];
      await gpuDone();
      for (let i = 0; i < n && !this.stopped; i++) {
        present(host_redraw);
        const submitted = performance.now();
        await gpuDone();
        ms.push(performance.now() - submitted);
      }
      return ms;
    };
    const p = this.stepping.then(run);
    this.stepping = p.catch(() => {
    });
    return p;
  }
  /** Preview (`mb.isolate`, `mb shot --readability`): draws the frame on
   *  screen again with the mb3d node `node` (and what hangs under it) left
   *  out of the camera's pass (0: none), and without the mb2d HUD unless
   *  `hud`; `plain` also without the retro look, fog, film and grade, brighter
   *  (for a silhouette mask). Presentation only, like `redrawTimed`. */
  redrawIsolated(node, hud, plain) {
    if (!this.held) return Promise.reject(new Error("mb.isolate: pause the session first (mb.suspend(), or restart with hold)"));
    const run = async () => {
      await gpuDone();
      present(() => host_redraw_isolated(node, hud, plain));
      await gpuDone();
    };
    const p = this.stepping.then(run);
    this.stepping = p.catch(() => {
    });
    return p;
  }
  /** The sensor readings that differ from what the guest saw last frame. */
  sampleSensors() {
    const s = this.sensors;
    const d = {};
    if (!s) return d;
    const tilt = s.tilt?.join() ?? "";
    if (tilt !== this.seen.tilt) {
      this.seen.tilt = tilt;
      d.tilt = s.tilt ? [...s.tilt] : null;
    }
    const motion = s.motion?.join() ?? "";
    if (motion !== this.seen.motion) {
      this.seen.motion = motion;
      d.motion = s.motion ? [...s.motion] : null;
    }
    const attitude = s.attitude?.join() ?? "";
    if (attitude !== this.seen.att) {
      this.seen.att = attitude;
      d.att = s.attitude ? [...s.attitude] : null;
    }
    if (s.loudness !== null && s.loudness !== this.seen.loud) {
      this.seen.loud = s.loudness;
      d.loud = s.loudness;
    }
    return d;
  }
  /** Plays one recorded frame per display frame; live input is ignored. */
  replayTick() {
    if (this.source.kind !== "replay") return;
    this.input.take();
    const f = this.source.log.frames[this.cursor];
    if (!f) {
      this.onReplayEnd?.();
      return;
    }
    if (!this.session.assets.tryPublishRecorded(f.assets ?? [])) return;
    this.cursor++;
    this.step(f.dt, f.events ?? [], f, f.calls, f.lost);
  }
  /** One frame: replay recorded lifecycle calls, deliver input, update, render,
   *  present. Returns what it cost (also kept for `mb.stats()`), or null if
   *  the session stopped. */
  step(dt, events, sensors, calls, lost = 0) {
    for (const c of calls ?? []) this.lifecycle(c);
    if (this.stopped) return null;
    this.session.beginFrame(dt, events, sensors, lost);
    if (sensors?.screen) host_set_logical(this.session.screen[0], this.session.screen[1]);
    host_step(dt);
    this.session.audio.frame(this.frameNow ?? performance.now(), this.session.gameTime, this.source.kind === "live" ? _Runtime.timeScale : 1);
    const update = this.timed("mb_update", () => this.guest.mb_update(dt));
    if (this.stopped) return null;
    this.frames++;
    host_begin_frame();
    const render = this.timed("mb_render", () => this.guest.mb_render());
    if (this.stopped) return null;
    let presentMs = null;
    let submitted = null;
    if (this.presenting || this.prerolling) present(host_prepare);
    if (this.presenting) {
      const t0 = performance.now();
      present(host_end_frame);
      submitted = performance.now();
      presentMs = submitted - t0;
      _Runtime.cpu("present", presentMs);
    }
    this.latency.submitted(submitted);
    if (this.hashes.length < MAX_FRAME_HASHES) this.hashes.push(host_draw_hash());
    const kept = presentMs !== null && this.stats.drawn;
    const drawn = kept ? JSON.parse(host_stats()) : {};
    const stat = { ...drawn, frame: this.frames, interval: this.frameInterval, mb_update: update, mb_render: render, present: presentMs, gpu: null };
    const pose2 = kept ? host_pose() : null;
    if (pose2 && pose2.length >= 9) {
      stat.camera = Array.from(pose2.subarray(0, 9));
      if (pose2.length >= 12) stat.track = Array.from(pose2.subarray(9, 12));
    }
    if (this.session.marks.length) stat.marks = this.session.marks;
    if (this.session.logs.length) stat.logs = this.session.logs;
    const over = kept ? overBudget(drawn) : [];
    if (over.length) {
      stat.over = over;
      stat.top = JSON.parse(host_top_draws(TOP_OVER));
    }
    this.stats.push(stat);
    return stat;
  }
  /** Runs a guest callback under the watchdogs; returns how long it took (ms). */
  timed(name, f, budget = CALLBACK_BUDGET_MS) {
    this.guest.__mb_fuel.value = FUEL_PER_CALLBACK;
    const t = performance.now();
    try {
      f();
    } catch (e) {
      this.stop();
      if (this.guest.__mb_fuel.value === 0) {
        post({ op: "watchdog", callback: `${name} (loop fuel exhausted)`, ms: performance.now() - t });
      } else {
        post({ op: "error", message: `${name} trapped: ${e instanceof Error ? e.message : String(e)}` });
      }
      return performance.now() - t;
    }
    const ms = performance.now() - t;
    _Runtime.cpu(name, ms);
    if (ms > budget && _Runtime.budgets) {
      this.stop();
      post({ op: "watchdog", callback: name, ms });
    }
    return ms;
  }
  stop() {
    this.stopped = true;
    cancelFrame(this.raf);
  }
  brief() {
    return { frames: this.frames, time: this.session.gameTime, hash: host_draw_hash(), replay: this.source.kind === "replay" };
  }
  /** Paused, held or not started: no frames run on their own. */
  get held() {
    return this.suspended || !this.running;
  }
  /** Watch mode (`mb serve --watch`): runs another run's recorded frames in
   *  this fresh, not yet started live session as fast as it can, recording
   *  them as its own, so the session carries on live from where that run
   *  was (with new code or assets). Compares the draw hash after mb_init and
   *  after every frame with that run's (`expected`) to tell whether it went
   *  the same way. Silent and unpresented until the last frame. */
  async fastForward(frames, expected, progress) {
    this.presenting = false;
    this.session.audio.fastForward(true);
    this.session.fastForwarding = true;
    let firstMismatch = expected.length > 0 && this.hashes[0] !== expected[0] ? 0 : null;
    let missingAssets = 0;
    let slice = performance.now();
    try {
      for (let i = 0; i < frames.length && !this.stopped; i++) {
        const f = frames[i];
        let assets = [];
        if (f.assets?.length) {
          assets = await this.session.assets.publishKnown(f.assets);
          missingAssets += f.assets.length - assets.length;
        }
        const sensors = {};
        if (f.tilt !== void 0) sensors.tilt = f.tilt;
        if (f.motion !== void 0) sensors.motion = f.motion;
        if (f.att !== void 0) sensors.att = f.att;
        if (f.loud !== void 0) sensors.loud = f.loud;
        if (f.screen !== void 0) sensors.screen = f.screen;
        this.recorder.frame(f.dt, f.events ?? [], assets, sensors, f.calls ?? [], f.lost ?? 0);
        this.step(f.dt, f.events ?? [], sensors, f.calls, f.lost);
        if (firstMismatch === null && i + 1 < expected.length && this.hashes[i + 1] !== expected[i + 1]) firstMismatch = i + 1;
        if (performance.now() - slice > 16) {
          progress?.(i + 1, frames.length);
          await yieldTask();
          slice = performance.now();
        }
      }
    } finally {
      this.presenting = true;
      this.session.audio.fastForward(false);
      this.session.fastForwarding = false;
    }
    if (!this.stopped) present(host_end_frame);
    return { frames: this.frames, of: frames.length, firstMismatch, compared: Math.min(expected.length, this.frames + 1), missingAssets };
  }
  /** Recorded frames a replay session has played (the next one to play). */
  get replayCursor() {
    return this.cursor;
  }
  /** Seeking a held replay session (main.ts `mb.seek`): plays its recorded
   *  frames up to frame `target` (that many played) as fast as it can,
   *  silent and unpresented, for at most `budgetMs`, then shows the last
   *  one. Resolves to whether it got there. Sound picks up where it would
   *  be once the session plays on: one-shots set off on the way are skipped,
   *  as watch mode's restore does. */
  async seekStep(target, budgetMs) {
    if (this.source.kind !== "replay") throw new Error("seekStep needs a replay session");
    const frames = this.source.log.frames;
    const end = Math.min(target, frames.length);
    const t0 = performance.now();
    let ran = 0;
    this.presenting = false;
    this.session.audio.fastForward(true);
    try {
      while (this.cursor < end && !this.stopped && performance.now() - t0 < budgetMs) {
        const f = frames[this.cursor];
        await this.session.assets.publishRecorded(f.assets ?? []);
        this.cursor++;
        this.step(f.dt, f.events ?? [], f, f.calls, f.lost);
        ran++;
      }
    } finally {
      this.presenting = true;
      this.session.audio.fastForward(false);
    }
    if (ran > 0 && !this.stopped) {
      present(host_prepare);
      present(host_end_frame);
    }
    return this.cursor >= end;
  }
  /** Headless (`mb replay`): plays a replay session's whole log as fast as it
   *  can, unpresented, waiting for each asset the run saw arrive. `onFrame`
   *  is told the 1-based number of the frame about to run. Stops early if the
   *  guest traps or runs out of fuel. On another build the log may name
   *  assets this one never asked for: those are skipped and counted
   *  (`missingAssets`), as watch mode's restore does, and the run goes on. */
  async replayAll(onFrame) {
    if (this.source.kind !== "replay") throw new Error("replayAll needs a replay session");
    this.presenting = false;
    const frames = this.source.log.frames;
    let missingAssets = 0;
    while (this.cursor < frames.length && !this.stopped) {
      const f = frames[this.cursor];
      if (f.assets?.length) missingAssets += f.assets.length - (await this.session.assets.publishKnown(f.assets)).length;
      this.cursor++;
      onFrame?.(this.frames + 1);
      this.step(f.dt, f.events ?? [], f, f.calls, f.lost);
    }
    return { ...this.brief(), missingAssets };
  }
  /** The draw hash after mb_init (0) and after each frame since, as far as kept. */
  frameHashes() {
    return this.hashes;
  }
  snapshot() {
    const log2 = this.source.kind === "replay" ? { ...this.source.log, frames: this.source.log.frames.slice(0, this.cursor) } : this.recorder.log;
    return { ...this.brief(), log: structuredClone(log2) };
  }
};
function gpuDone() {
  return Promise.race([host_gpu_done(), new Promise((r) => setTimeout(r, GPU_WAIT_MAX_MS))]);
}
function cancelFrame(id) {
  if (typeof cancelAnimationFrame !== "undefined") cancelAnimationFrame(id);
}
function yieldTask() {
  return new Promise((resolve) => {
    const ch = new MessageChannel();
    ch.port1.onmessage = () => resolve();
    ch.port2.postMessage(null);
  });
}
function hexBytes(hex) {
  const out = new Uint8Array(16);
  for (let i = 0; i < 16; i++) out[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16) || 0;
  return out;
}
var MAX_PARAMS = 32;
function checkParams(p) {
  const out = {};
  for (const [k, v] of Object.entries(p ?? {}).slice(0, MAX_PARAMS)) {
    if (k.length > 0 && k.length <= 64 && typeof v === "string" && v.length <= 256) out[k] = v;
  }
  return out;
}
async function createSession(page2, source, seedOverride, from, paramsOverride) {
  const { boot, manifest: m } = page2;
  const log2 = source.kind === "replay" ? source.log : from;
  const params = hasNative ? {} : checkParams(log2 ? log2.params : paramsOverride ?? page2.params);
  const seed = BigInt(log2?.seed ?? seedOverride ?? boot.seed);
  const dailySeed = BigInt(log2?.dailySeed ?? boot.dailySeed);
  const store = log2?.store ?? { ...page2.store };
  const playerId = log2?.playerId ?? boot.playerId;
  const locale = log2?.locale ?? boot.locale;
  const screen2 = new Float32Array(log2 ? log2.screen : page2.screen);
  host_new_session();
  host_set_stdlib(boot.manifest.stdlib.mb3d ?? 0);
  host_set_logical(screen2[0], screen2[1]);
  page2.audio.newSession();
  const session = new Session(page2.audio, seed, dailySeed, hexBytes(playerId), locale, screen2, page2.gameBase, store, page2.assets);
  session.replaying = source.kind === "replay";
  for (const [k, v] of Object.entries(params)) session.params.set(k, v);
  session.observer = page2.observer ?? null;
  const live = source.kind === "live";
  if (live) {
    session.play = page2.play ?? null;
    session.onHaptic = (k) => page2.gamepads?.rumble(k);
  }
  session.onStore = (key, value) => {
    if (value === null) delete page2.store[key];
    else page2.store[key] = value;
  };
  const instance = await WebAssembly.instantiate(page2.module, { mb: buildImports(session, page2.host, m) });
  const guest = instance.exports;
  if (!(guest.__mb_fuel instanceof WebAssembly.Global)) throw new Error("game.wasm is not loop-metered; refusing to run it");
  session.memory = () => guest.memory;
  const recorder = new Recorder({
    game: m.id,
    ...boot.bundle ? { bundle: boot.bundle } : {},
    seed: seed.toString(),
    dailySeed: dailySeed.toString(),
    store,
    playerId,
    locale,
    screen: Array.from(screen2),
    ...Object.keys(params).length ? { params } : {}
  });
  const view = source.kind === "live" ? { screen: page2.screen, adaptive: page2.adaptive === true, pending: page2.pending } : null;
  const rt = new Runtime(guest, session, page2.input, recorder, page2.gamepads, page2.sensors, page2.stats, page2.latency, source, view, live ? page2.play ?? null : null);
  rt.init();
  return rt;
}

// src/dev.ts
var SAVED_KEY = "mb.dev.saved";
var PAGE_KEY = "mb.dev.page";
var AUTO_KEY = "mb.dev.autoRestore";
var MAX_SAVED_CHARS = 4e6;
function storage(kind) {
  try {
    return kind === "session" ? window.sessionStorage : window.localStorage;
  } catch {
    return null;
  }
}
var DevClient = class {
  /** Stable for the tab across reloads, so `mb serve` counts it once. */
  page;
  hooks = null;
  failedBoot = false;
  /** The code build this page runs, and the latest build it has applied. */
  codeBuild = 0;
  build = 0;
  /** What the server last said. */
  latest = { server: "", build: 0, codeBuild: 0 };
  /** Synced with the server at least once since this page loaded. */
  synced = false;
  /** The build last reported running. */
  reported = -1;
  chain = Promise.resolve();
  saved = null;
  lastRestore = null;
  connected = false;
  retry = 250;
  ui = new Ui();
  logBudget = { n: 0, t: 0 };
  constructor() {
    const ss = storage("session");
    let id = ss?.getItem(PAGE_KEY) ?? "";
    if (!/^[a-z0-9]{8,32}$/.test(id)) {
      id = Math.random().toString(36).slice(2, 12) + Date.now().toString(36);
      ss?.setItem(PAGE_KEY, id);
    }
    this.page = id;
    taps.push((m) => this.tap(m));
    this.connect();
  }
  get auto() {
    const q2 = new URLSearchParams(location.search).get("restore");
    if (q2 !== null) return q2 !== "off" && q2 !== "0";
    return storage("local")?.getItem(AUTO_KEY) === "1";
  }
  // --- the connection ----------------------------------------------------------
  connect() {
    let ws;
    try {
      ws = new WebSocket(`ws://${location.host}/__mb/ws?page=${this.page}`);
    } catch {
      this.reconnectLater();
      return;
    }
    ws.onopen = () => {
      this.connected = true;
      this.retry = 250;
      this.ui.pill(null);
    };
    ws.onmessage = (e) => {
      try {
        this.message(JSON.parse(String(e.data)));
      } catch (err) {
        console.error("[mb dev]", err);
      }
    };
    ws.onclose = () => {
      if (this.connected) this.ui.pill("disconnected from mb serve; retrying\u2026", "warn");
      this.connected = false;
      this.reconnectLater();
    };
  }
  reconnectLater() {
    setTimeout(() => this.connect(), this.retry);
    this.retry = Math.min(this.retry * 2, 2e3);
  }
  report(body) {
    void fetch("/__mb/page", { method: "POST", body: JSON.stringify({ page: this.page, ...body }), keepalive: true }).catch(() => {
    });
  }
  /** Errors and the game's warnings, forwarded to the terminal running `mb serve`. */
  tap(m) {
    if (m.op === "error" && m.message.startsWith("boot failed") && m.message.includes(": 503")) {
      this.ui.runtimeError("there's no good build to run yet. This page reloads once there is.");
    } else if (m.op === "error" || m.op === "watchdog" || m.op === "gpu-lost") {
      const text = m.op === "watchdog" ? `watchdog: ${m.callback} took ${m.ms.toFixed(0)} ms` : m.message;
      this.ui.runtimeError(text);
      this.report({ type: "error", build: this.build, message: text });
    } else if (m.op === "log" && m.level >= 2 && !m.msg.startsWith("runtime:")) {
      const now = performance.now();
      if (now - this.logBudget.t > 1e3) this.logBudget = { n: 0, t: now };
      if (this.logBudget.n++ < 10) this.report({ type: "log", message: m.msg });
    }
  }
  // --- what the server says ------------------------------------------------------
  message(m) {
    const n = (k) => typeof m[k] === "number" ? m[k] : 0;
    switch (m.type) {
      case "hello": {
        this.latest = { server: String(m.server), build: n("build"), codeBuild: n("codeBuild") };
        if (m.failure) this.failed(m.failure);
        else this.ui.buildError(null);
        if (m.state === "building") this.ui.pill("building\u2026");
        if (this.hooks) this.enqueue(() => this.sync());
        else if (this.failedBoot && m.state === "ok") this.reloadPage();
        return;
      }
      case "building":
        this.ui.pill(`build ${n("build")}: building\u2026`);
        return;
      case "failed":
        this.failed(m);
        return;
      case "reload":
        this.latest.build = n("build");
        this.latest.codeBuild = n("codeBuild");
        this.ui.buildError(null);
        if (!this.hooks) {
          if (this.failedBoot) this.reloadPage();
          return;
        }
        if (m.kind === "page") this.enqueue(async () => this.reloadPage());
        else this.enqueue(() => this.applyCode(n("build")));
        return;
      case "assets":
        this.latest.build = n("build");
        if (this.hooks) this.enqueue(() => this.applyAssets(n("build"), m.paths ?? []));
        return;
    }
  }
  failed(f) {
    this.ui.pill(null);
    this.ui.buildError(f, this.codeBuild);
    console.error(`[mb dev] build ${f.build} failed: ${f.summary}`);
  }
  enqueue(f) {
    this.chain = this.chain.then(f).catch((e) => {
      console.error("[mb dev]", e);
      this.ui.pill(`reload failed: ${e instanceof Error ? e.message : String(e)}`, "warn");
    });
  }
  /** Called once the page has booted and its first session runs. */
  attach(hooks) {
    this.hooks = hooks;
    this.codeBuild = hooks.boot?.build ?? 0;
    this.build = this.codeBuild;
    window.addEventListener("pagehide", () => this.capture());
    const stored = this.load();
    if (stored && stored.game === hooks.game) this.saved = stored;
    this.enqueue(async () => {
      if (hooks.replay === "played") this.ui.offer(null);
      else if (hooks.replay) await this.doRestore(hooks.replay, "replay");
      else if (this.saved && this.auto) await this.doRestore(this.saved, "reload");
      else if (this.saved) this.ui.offer(this.saved, () => this.enqueue(() => this.doRestore(this.saved, "reload")), () => this.setAuto());
      await this.sync();
    });
  }
  /** The page couldn't boot: wait for a good build, then reload. */
  bootFailed() {
    this.failedBoot = true;
  }
  /** Catches up with the server (after booting, and after reconnecting):
   *  takes its newest code if this page is behind, and reports what runs. */
  async sync() {
    const l = this.latest;
    if (!l.server || !this.hooks) return;
    const boot = this.hooks.boot;
    if (boot && boot.server !== l.server) {
      this.reloadPage();
      return;
    }
    const first = !this.synced;
    this.synced = true;
    if (first && l.codeBuild === this.codeBuild) this.build = l.build;
    if (l.codeBuild > this.codeBuild || l.build > this.build) {
      await this.applyCode(l.codeBuild, l.build > l.codeBuild);
    } else if (this.reported !== this.build) {
      this.running();
    }
  }
  running(restored) {
    const frames = this.hooks?.frames() ?? 0;
    this.reported = this.build;
    this.report({ type: "running", build: this.build, frames, restored: restored ?? null });
  }
  // --- reloads ---------------------------------------------------------------------
  capture() {
    const s = this.hooks?.save() ?? null;
    if (s) {
      this.saved = s;
      this.store(s);
    }
    return s;
  }
  store(s) {
    const ss = storage("session");
    if (!ss) return;
    let json = JSON.stringify(s);
    if (json.length > MAX_SAVED_CHARS) json = JSON.stringify({ ...s, hashes: [] });
    try {
      if (json.length > MAX_SAVED_CHARS) throw new Error("too long");
      ss.setItem(SAVED_KEY, json);
    } catch {
      ss.removeItem(SAVED_KEY);
      console.warn(`[mb dev] session too long to keep across a page reload (${s.frames} frames); mb.restore() works until then`);
    }
  }
  load() {
    try {
      const v = JSON.parse(storage("session")?.getItem(SAVED_KEY) ?? "null");
      return v && v.v === 1 && Array.isArray(v.log?.frames) ? v : null;
    } catch {
      return null;
    }
  }
  reloadPage() {
    this.capture();
    location.reload();
  }
  /** New code: compile it, swap it in, start fresh (or restore). `allAssets`:
   *  asset changes may have been missed while disconnected. */
  async applyCode(build, allAssets = false) {
    const hooks = this.hooks;
    const t = performance.now();
    const res = await fetch(`../game/game.wasm?build=${build}`, { cache: "no-store" });
    if (!res.ok) throw new Error(`game.wasm: HTTP ${res.status}`);
    const module = await WebAssembly.compile(await res.arrayBuffer());
    const saved = this.capture() ?? this.saved;
    hooks.swapCode(module);
    if (allAssets) hooks.invalidate(null);
    this.codeBuild = build;
    this.build = Math.max(build, this.latest.build);
    this.ui.runtimeError(null);
    if (saved && this.auto) {
      await this.doRestore(saved, "code");
      return;
    }
    await hooks.fresh(saved?.held ?? false);
    const ms = performance.now() - t;
    this.ui.pill(`build ${build} running (${ms.toFixed(0)} ms)`, "ok", 2500);
    console.info(`[mb dev] build ${build} running (swapped in ${ms.toFixed(0)} ms)`);
    if (saved) this.ui.offer(saved, () => this.enqueue(() => this.doRestore(saved, "code")), () => this.setAuto());
    this.running();
  }
  /** Changed assets: refetch them and replay the session to the same frame. */
  async applyAssets(build, paths) {
    const hooks = this.hooks;
    const saved = this.capture();
    hooks.invalidate(paths);
    this.build = build;
    this.ui.runtimeError(null);
    if (saved) {
      await this.doRestore(saved, "assets");
    } else {
      await hooks.fresh(this.saved?.held ?? false);
      this.ui.pill(`build ${build}: assets updated`, "ok", 2500);
      this.running();
    }
  }
  async doRestore(saved, why) {
    const hooks = this.hooks;
    this.ui.offer(null);
    const t = performance.now();
    const r = await hooks.restore(saved, (done, total) => this.ui.pill(`restoring\u2026 ${Math.round(100 * done / total)}%`));
    const ms = performance.now() - t;
    this.lastRestore = { ...r, at: Date.now() };
    const where = `frame ${r.frames}${r.frames < r.of ? ` of ${r.of} (the game stopped)` : ""}`;
    const how = why === "replay" ? "a run from elsewhere keeps no per-frame draw hashes, so where it differs on this code isn't known; after an edit it's replayed again and compared with this" : r.compared === 0 ? "no hashes were kept to compare" : r.firstMismatch === null ? "same as before" : `differs from the old run from frame ${r.firstMismatch} (changed drawing or logic), so this may not be exactly where you were`;
    const lead = why === "assets" ? "assets updated, replayed to" : why === "replay" ? "replayed the run to" : "restored to";
    this.ui.pill(`${lead} ${where}: ${how}`, r.firstMismatch === null ? "ok" : "warn", r.firstMismatch === null ? 3e3 : 8e3);
    console.info(`[mb dev] ${lead} ${where} in ${ms.toFixed(0)} ms: ${how}`);
    this.running({ ...r, why });
    return r;
  }
  setAuto() {
    storage("local")?.setItem(AUTO_KEY, "1");
    this.ui.pill("always restoring after a reload (?restore=off to stop)", "ok", 3e3);
  }
  // --- console ---------------------------------------------------------------------
  /** `mb.restore()`. */
  restore() {
    return new Promise((resolve, reject) => {
      this.enqueue(async () => {
        const s = this.saved ?? this.load();
        if (!this.hooks) return reject(new Error("the game isn't running"));
        if (!s) return reject(new Error("no earlier session to restore (it must have run at least one frame)"));
        resolve(await this.doRestore(s, "console"));
      });
    });
  }
  /** `mb.dev()`. */
  status() {
    return {
      page: this.page,
      connected: this.connected,
      build: this.build,
      codeBuild: this.codeBuild,
      serverBuild: this.latest.build,
      autoRestore: this.auto,
      saved: this.saved ? { frames: this.saved.frames, time: this.saved.time, held: this.saved.held } : null,
      lastRestore: this.lastRestore
    };
  }
};
function startDev() {
  return new DevClient();
}
var FONT = "12px/1.4 ui-monospace, SFMono-Regular, Menlo, Consolas, monospace";
var Ui = class {
  root;
  pillEl;
  pillTimer = 0;
  errorEl;
  runtimeEl;
  offerEl;
  offerTimer = 0;
  dismissed = -1;
  constructor() {
    const el = (css) => {
      const d = document.createElement("div");
      d.style.cssText = css;
      return d;
    };
    this.root = el("position:fixed;inset:0;pointer-events:none;z-index:30");
    this.pillEl = el(`position:absolute;top:8px;right:8px;max-width:70vw;padding:4px 10px;border-radius:12px;font:${FONT};color:#fff;background:rgba(0,0,0,.75);display:none`);
    this.offerEl = el(
      `position:absolute;top:40px;left:50%;transform:translateX(-50%);max-width:92vw;padding:8px 12px;border-radius:10px;font:${FONT};color:#fff;background:rgba(20,40,90,.92);box-shadow:0 2px 12px rgba(0,0,0,.5);pointer-events:auto;display:none`
    );
    this.errorEl = el(
      `position:absolute;left:0;right:0;bottom:0;max-height:65vh;overflow:auto;padding:10px 14px;font:${FONT};color:#ffd9d9;background:rgba(40,0,0,.93);border-top:3px solid #e33;white-space:pre-wrap;pointer-events:auto;display:none`
    );
    this.runtimeEl = el(
      `position:absolute;left:0;right:0;top:0;max-height:40vh;overflow:auto;padding:8px 14px;font:${FONT};color:#fff3cf;background:rgba(50,30,0,.93);border-bottom:3px solid #fa0;white-space:pre-wrap;pointer-events:auto;display:none`
    );
    this.root.append(this.runtimeEl, this.pillEl, this.offerEl, this.errorEl);
    const mount = () => document.body.append(this.root);
    if (document.body) mount();
    else window.addEventListener("DOMContentLoaded", mount);
  }
  pill(text, tone = "info", hideAfter = 0) {
    clearTimeout(this.pillTimer);
    if (text === null) {
      this.pillEl.style.display = "none";
      return;
    }
    this.pillEl.textContent = text;
    this.pillEl.style.background = tone === "ok" ? "rgba(10,90,40,.85)" : tone === "warn" ? "rgba(130,70,0,.9)" : "rgba(0,0,0,.75)";
    this.pillEl.style.display = "block";
    if (hideAfter) this.pillTimer = window.setTimeout(() => this.pillEl.style.display = "none", hideAfter);
  }
  closeButton(onClick) {
    const b = document.createElement("button");
    b.textContent = "\xD7";
    b.title = "Hide";
    b.style.cssText = "float:right;margin-left:12px;border:0;background:none;color:inherit;font:16px system-ui;cursor:pointer";
    b.onclick = onClick;
    return b;
  }
  /** The compiler's errors, over the game (null clears them). */
  buildError(f, running = 0) {
    if (!f) {
      this.errorEl.style.display = "none";
      return;
    }
    if (this.dismissed === f.build) return;
    this.errorEl.replaceChildren();
    const head = document.createElement("div");
    head.style.cssText = "font-weight:bold;color:#fff;margin-bottom:6px";
    head.textContent = `Build ${f.build} failed${running ? `; still running build ${running}` : ""}. Fix it and save.`;
    head.prepend(this.closeButton(() => (this.dismissed = f.build, this.errorEl.style.display = "none")));
    const body = document.createElement("div");
    body.textContent = f.text || f.summary;
    this.errorEl.append(head, body);
    this.errorEl.style.display = "block";
  }
  /** A trap, watchdog or failed boot (null clears it). */
  runtimeError(text) {
    if (text === null) {
      this.runtimeEl.style.display = "none";
      return;
    }
    this.runtimeEl.replaceChildren(this.closeButton(() => this.runtimeEl.style.display = "none"), `The game stopped: ${text}`);
    this.runtimeEl.style.display = "block";
  }
  /** Offers to restore `s` (null withdraws the offer). */
  offer(s, restore2, always) {
    clearTimeout(this.offerTimer);
    if (!s || !restore2) {
      this.offerEl.style.display = "none";
      return;
    }
    const button = (label, onClick) => {
      const b = document.createElement("button");
      b.textContent = label;
      b.style.cssText = "margin-left:8px;padding:2px 8px;border-radius:6px;border:1px solid #9bf;background:#2a5bd7;color:#fff;font:inherit;cursor:pointer";
      b.onclick = () => {
        this.offerEl.style.display = "none";
        onClick();
      };
      return b;
    };
    const text = document.createElement("span");
    text.textContent = `Back to where you were? Replays your last session (${s.frames} frames, ${s.time.toFixed(1)} s${s.held ? ", held" : ""}) into this build.`;
    this.offerEl.replaceChildren(text, button("Restore", restore2));
    if (always) this.offerEl.append(button("Always", () => (always(), restore2())));
    this.offerEl.append(this.closeButton(() => this.offerEl.style.display = "none"));
    this.offerEl.style.display = "block";
    this.offerTimer = window.setTimeout(() => this.offerEl.style.display = "none", 2e4);
  }
};

// src/main.ts
var TOP_STATS = 8;
var MAX_FILL_ASPECT = 2.4;
function isAdaptive(boot) {
  const m = boot.manifest;
  return (m.orientation === "landscape" || m.orientation === "any" || m.fit === "fill") && !builtBefore08(boot.sdk);
}
function builtBefore08(sdk) {
  if (sdk === void 0) return false;
  const [major, minor] = (sdk ?? "0.0").split(".").map((n) => parseInt(n, 10) || 0);
  return (major ?? 0) === 0 && (minor ?? 0) < 8;
}
function viewOrientation() {
  return window.innerWidth > window.innerHeight ? "landscape" : "portrait";
}
var adaptiveLandscape = false;
function fit(canvas, m, view) {
  let [lw, lh] = m.logical_size;
  if (m.orientation === "any" && viewOrientation() === "landscape") [lw, lh] = [lh, lw];
  if (m.orientation === "landscape" && viewOrientation() === "portrait" && adaptiveLandscape) [lw, lh] = [lh, lw];
  const cssW = window.innerWidth;
  const cssH = window.innerHeight;
  let scale = Math.min(cssW / lw, cssH / lh) || 1;
  if (m.fit === "fill" && cssW > 0 && cssH > 0) {
    let w = cssW / scale;
    let h = cssH / scale;
    if (w / h > MAX_FILL_ASPECT) w = h * MAX_FILL_ASPECT;
    if (h / w > MAX_FILL_ASPECT) h = w * MAX_FILL_ASPECT;
    lw = Math.min(4096, Math.max(lw, Math.round(w)));
    lh = Math.min(4096, Math.max(lh, Math.round(h)));
    scale = Math.min(cssW / lw, cssH / lh) || 1;
  }
  const offsetX = (cssW - lw * scale) / 2;
  const offsetY = (cssH - lh * scale) / 2;
  const k = Math.min(window.devicePixelRatio || 1, 2 / scale);
  const size = [Math.max(1, Math.round(cssW * k)), Math.max(1, Math.round(cssH * k))];
  canvas.width = size[0];
  canvas.height = size[1];
  const ins = view.insets;
  const toLogical = (css, off) => Math.max(0, css - off) / scale;
  const screen2 = new Float32Array([
    lw,
    lh,
    toLogical(ins.top, offsetY),
    toLogical(ins.right, offsetX),
    toLogical(Math.max(ins.bottom, view.stripHeight), offsetY),
    toLogical(ins.left, offsetX),
    k * scale,
    0
  ]);
  return { layout: { scale, offsetX, offsetY, height: cssH, stripHeight: view.stripHeight }, screen: screen2, size };
}
function drawOverlays(layout, screen2, mic, bannerTop) {
  const root = document.createElement("div");
  root.style.cssText = "position:fixed;inset:0;pointer-events:none;z-index:10";
  document.body.append(root);
  const draw = () => {
    const l = layout();
    const lw = screen2[0] ?? 360, lh = screen2[1] ?? 640, insetTop = screen2[2] ?? 0, insetBottom = screen2[4] ?? 0;
    const stripBottom = lh - Math.max(insetBottom, 70) - 8;
    const box = (x, y, w, h, label) => `<div style="position:absolute;left:${l.offsetX + x * l.scale}px;top:${l.offsetY + y * l.scale}px;width:${w * l.scale}px;height:${h * l.scale}px;background:rgba(255,0,128,.25);outline:1px dashed #f08;color:#fff;font:10px system-ui;padding:2px;box-sizing:border-box">${label}</div>`;
    root.innerHTML = box(0, lh - 70, lw, 70, "card: title + creator") + box(lw - 56, lh * 0.45, 56, lh * 0.55 - 70, "card: buttons") + box(56, stripBottom - 96, lw - 112, 96, "game-over card: challenge strip") + (bannerTop ? box(72, insetTop, lw - 144, 110, "replay card: Beat \u2026 (top)") : box(0, lh - 230, lw - 56, 160, "replay card: Beat \u2026")) + box(8, insetTop + 4, mic ? 110 : 64, 44, mic ? "pause pill + mic badge (play)" : "pause pill (play)");
  };
  draw();
  return draw;
}
var SEEK_SLICE_MS = 12;
var SEEK_HEARTBEAT_MS = 250;
var frameTimes = /* @__PURE__ */ new WeakMap();
function frameAt(frames, seconds) {
  let t = frameTimes.get(frames);
  if (!t) {
    t = new Float64Array(frames.length + 1);
    for (let i = 0; i < frames.length; i++) t[i + 1] = t[i] + frames[i].dt;
    frameTimes.set(frames, t);
  }
  const want = Number.isFinite(seconds) ? seconds : 0;
  let lo = 1;
  let hi = frames.length;
  while (lo < hi) {
    const mid = lo + hi >> 1;
    if (t[mid] >= want) hi = mid;
    else lo = mid + 1;
  }
  return Math.max(Math.min(lo, frames.length), Math.min(1, frames.length));
}
function nextPaint() {
  return new Promise((resolve) => {
    const timer = setTimeout(resolve, 32);
    requestAnimationFrame(() => {
      clearTimeout(timer);
      resolve();
    });
  });
}
var dev = !hasNative && document.querySelector('meta[name="mb-dev"]') ? startDev() : null;
async function main() {
  const t0 = performance.now();
  window.addEventListener("error", (e) => post({ op: "error", message: String(e.message) }));
  window.addEventListener("unhandledrejection", (e) => post({ op: "error", message: String(e.reason) }));
  const boot = await loadBoot();
  const m = boot.manifest;
  const canvas = document.getElementById("game");
  const view = { insets: { ...boot.insets }, stripHeight: boot.stripHeight };
  const adaptive = isAdaptive(boot);
  adaptiveLandscape = adaptive;
  let { layout, screen: screen2 } = fit(canvas, m, view);
  const host = await __wbg_init({ module_or_path: new URL("mb_host_bg.wasm", import.meta.url) });
  await host_init(canvas, screen2[0], screen2[1]);
  const gameBase = boot.gameBase ?? "../game/";
  const module = await WebAssembly.compile(await (await fetch(`${gameBase}game.wasm`)).arrayBuffer());
  const input = new Input(canvas, () => layout, m.inputs.includes("touch"), m.inputs.includes("mouse"));
  const play = new Play(viewOrientation);
  input.activity = play;
  const keyboard = m.inputs.includes("keyboard") ? new Keyboard((e, stamp) => input.push(e, stamp), play, boot.nativeKeys !== true) : null;
  const page2 = {
    boot,
    manifest: m,
    gameBase,
    module,
    host,
    audio: new Audio(),
    input,
    gamepads: m.inputs.includes("gamepad") ? new Gamepads(boot.nativePads === true, play) : null,
    sensors: ["tilt", "motion", "attitude", "loudness"].some((x) => m.sensors.includes(x)) ? new Sensors(
      {
        tilt: m.sensors.includes("tilt"),
        motion: m.sensors.includes("motion"),
        attitude: m.sensors.includes("attitude"),
        loudness: m.sensors.includes("loudness")
      },
      hasNative ? "native" : boot.web && Sensors.deviceHasMotion() ? "device" : "emulate",
      // Preview (SPEC §9): ?sensors=none previews a device without tilt, motion or attitude.
      !hasNative && new URLSearchParams(location.search).get("sensors") === "none"
    ) : null,
    screen: screen2,
    store: { ...boot.store ?? {} },
    assets: /* @__PURE__ */ new Map(),
    stats: new Stats(!hasNative),
    latency: new Latency(),
    params: hasNative ? {} : checkParams(Object.fromEntries(new URLSearchParams(location.search).getAll("param").map((p) => [p.split("=")[0] ?? "", p.slice(p.indexOf("=") + 1)]))),
    adaptive,
    play,
    pending: []
  };
  input.latency = page2.latency;
  const sampleKind = m.sensors.includes("attitude") ? "attitude" : "tilt";
  if (page2.sensors) page2.sensors.onEmulatedTilt = (stamp) => page2.latency.tilt(stamp, performance.now(), sampleKind);
  let drawOverlayLayer = null;
  const refit = () => {
    const f = fit(canvas, m, view);
    layout = f.layout;
    if (adaptive) page2.screen.set(f.screen);
    host_resize(f.size[0], f.size[1]);
    present(host_redraw);
    drawOverlayLayer?.();
  };
  window.addEventListener("resize", refit);
  let rt;
  let lagWorst = 0;
  let lagCount = 0;
  let lagLogged = 0;
  const padLag = (ms) => {
    if (ms <= 50) return;
    lagWorst = Math.max(lagWorst, ms);
    lagCount++;
    const now = performance.now();
    if (now - lagLogged < 1e3) return;
    lagLogged = now;
    post({ op: "log", level: 1, msg: `runtime: controller state reached the page ${Math.round(lagWorst)} ms after native sent it (${lagCount} late)` });
    lagWorst = 0;
    lagCount = 0;
  };
  let generation = 0;
  let switching = Promise.resolve();
  const serially = (f) => {
    const p = switching.then(f);
    switching = p.catch(() => {
    });
    return p;
  };
  const newSession = async (opts) => {
    const source = opts.replay ? { kind: "replay", log: opts.replay, loop: opts.loop ?? true } : { kind: "live" };
    const next = await createSession(page2, source, opts.seed, void 0, opts.params);
    if (source.kind === "replay") {
      next.onReplayEnd = () => {
        if (source.loop) void restart({ ...opts, paused: false });
        else next.suspend();
      };
    }
    return next;
  };
  let replayOpts = null;
  const restart = (opts) => {
    endSeek();
    if (opts.resume) {
      const log2 = opts.resume;
      replayOpts = null;
      return serially(() => resumeRun(log2, opts.hold ?? true));
    }
    return serially(async () => {
      rt?.stop();
      const next = await newSession(opts);
      rt = next;
      replayOpts = opts.replay ? opts : null;
      if (opts.hold) next.startHeld();
      else if (opts.paused) next.startPaused();
      else next.start();
      post({ op: "restarted", replay: next.source.kind === "replay" });
    });
  };
  let seekTarget = null;
  let seeking = null;
  let resumeAfterSeek = false;
  const endSeek = () => {
    generation++;
    seekTarget = null;
    seeking = null;
    resumeAfterSeek = false;
  };
  const seek = (seconds) => {
    if (rt?.source.kind !== "replay" || !replayOpts) return Promise.reject(new Error("mb.seek: the session isn't a replay"));
    seekTarget = frameAt(rt.source.log.frames, seconds);
    if (!seeking) {
      const gen = generation;
      const p = runSeek(gen).finally(() => {
        if (seeking === p) seeking = null;
      });
      seeking = p;
    }
    return seeking;
  };
  const runSeek = async (gen) => {
    if (!rt.held) {
      rt.suspend();
      resumeAfterSeek = true;
    }
    let beat = performance.now();
    while (gen === generation && seekTarget !== null) {
      const target = seekTarget;
      if (target < rt.replayCursor) {
        const opts = replayOpts;
        const fresh = await serially(async () => {
          if (gen !== generation) return false;
          const next = await newSession(opts);
          if (gen !== generation) {
            next.stop();
            return false;
          }
          rt.stop();
          rt = next;
          next.startHeld();
          return true;
        });
        if (!fresh) break;
      }
      const there = await rt.seekStep(target, SEEK_SLICE_MS);
      if (gen !== generation) break;
      if (performance.now() - beat > SEEK_HEARTBEAT_MS) {
        beat = performance.now();
        post({ op: "heartbeat", frame: rt.replayCursor, time: rt.session.gameTime });
      }
      if (there && seekTarget === target) seekTarget = null;
      else await nextPaint();
    }
    const log2 = rt.source.kind === "replay" ? rt.source.log : null;
    const result = { frame: rt.replayCursor, of: log2?.frames.length ?? 0, time: rt.session.gameTime };
    if (gen === generation) {
      post({ op: "seeked", ...result });
      if (resumeAfterSeek) {
        resumeAfterSeek = false;
        rt.resume();
      }
    }
    return result;
  };
  const resumeRun = async (log2, hold) => {
    rt?.stop();
    const next = await createSession(page2, { kind: "live" }, void 0, log2);
    rt = next;
    const r = await next.fastForward(log2.frames, []);
    if (rt !== next) return;
    page2.pending?.push(...heldAtEnd(log2));
    if (next.session.round === 1) next.session.play?.start();
    if (hold) next.startHeld();
    else next.start();
    post({ op: "restarted", replay: false, resumed: { frames: r.frames, of: r.of, missingAssets: r.missingAssets } });
  };
  const restore2 = async (saved, progress) => {
    endSeek();
    replayOpts = null;
    rt?.stop();
    const next = await createSession(page2, { kind: "live" }, void 0, saved.log);
    rt = next;
    const result = await next.fastForward(saved.log.frames, saved.hashes, progress);
    if (rt === next) {
      if (saved.held) next.startHeld();
      else next.start();
      post({ op: "restarted", replay: false });
      const round = next.session.round;
      if (round === 1) next.session.play?.start();
      if (round !== 0) post({ op: "round", state: round });
    }
    return result;
  };
  page2.audio.setPlayback(boot.soundOn === true);
  page2.audio.setUserMuted(boot.muted === true);
  window.mb = {
    sound: (on) => page2.audio.setPlayback(on),
    mute: (on) => page2.audio.setUserMuted(on),
    suspend: (opts) => {
      resumeAfterSeek = false;
      rt.suspend(opts?.quiet === true);
    },
    // A seek holds the session until it lands, then plays on.
    resume: () => {
      if (seeking) resumeAfterSeek = true;
      else rt.resume();
    },
    seek: (seconds) => seek(seconds),
    snapshot: () => rt.snapshot(),
    resize: (json) => {
      const v = typeof json === "string" ? JSON.parse(json) : json ?? {};
      if (v.insets) view.insets = { ...view.insets, ...v.insets };
      if (typeof v.stripHeight === "number") view.stripHeight = v.stripHeight;
      refit();
    },
    pads: (json, sentAt) => {
      if (typeof sentAt === "number") padLag(Date.now() - sentAt);
      page2.gamepads?.setNative(typeof json === "string" ? JSON.parse(json) : json);
    },
    key: (down, code) => keyboard?.key(down ? 5 : 6, code, performance.now()),
    inputs: (on) => {
      if (!on) page2.pending?.push(...keyboard?.releaseAll() ?? [], ...page2.gamepads?.releaseAll() ?? []);
      page2.gamepads?.setEnabled(on);
      if (keyboard) keyboard.enabled = on;
      if (on) rt?.inputsOn();
    },
    padTap: (button = 0, index = 0) => page2.gamepads?.tap(button, index),
    restart: (json) => restart(JSON.parse(json)),
    tap: (x, y) => input.tapAt(x, y),
    sensors: (...a) => {
      const s = page2.sensors;
      if (!s) return;
      if (a.length === 0) s.on();
      else if (a[0] === null) s.off();
      else {
        const received = performance.now();
        const v = (i) => a[i] ?? NaN;
        s.set(v(0), v(1), v(2), v(3), v(4), v(5), a.slice(7, 14).map((x) => x ?? NaN));
        if (s.want.tilt || s.want.attitude) page2.latency.tilt(received - (Number.isFinite(v(6)) ? v(6) : 0), received, sampleKind);
      }
    },
    presented: (frame, t) => page2.latency.presented(frame, t),
    latency: (on) => {
      if (on !== void 0 && on !== page2.latency.enabled) {
        page2.latency.clear();
        page2.latency.enabled = on;
      }
      return page2.latency.report();
    },
    loudness: (v) => page2.sensors?.external(v),
    step: (n = 1, opts) => rt.stepFrames(n, opts?.paced ?? true, opts?.dt ?? 1 / 60, opts?.until),
    gpu: (n = 30) => rt.redrawTimed(Math.max(1, Math.min(1e3, n | 0))),
    isolate: (node = 0, opts) => rt.redrawIsolated(node >>> 0, opts?.hud ?? true, opts?.plain ?? false),
    audio: () => page2.audio.debug(),
    verify: async () => {
      rt.suspend();
      const live = rt.snapshot();
      const expected = [...rt.frameHashes()];
      await restart({ replay: live.log, hold: true });
      let firstMismatch = rt.frameHashes()[0] === expected[0] ? null : 0;
      for (let i = 1; i <= live.frames; i++) {
        const b = await rt.stepFrames(1, false);
        if (b.frames !== i) break;
        if (firstMismatch === null && i < expected.length && b.hash !== expected[i]) firstMismatch = i;
      }
      const end = rt.brief();
      return { frames: live.frames, live: live.hash, replay: end.hash, match: end.frames === live.frames && end.hash === live.hash, firstMismatch };
    },
    restore: () => dev ? dev.restore() : Promise.reject(new Error("mb.restore() needs mb serve --watch")),
    dev: () => dev?.status() ?? null,
    speed: (k) => {
      Runtime.timeScale = Math.max(0.01, Math.min(4, k));
    },
    debugGpu,
    save: (name, data) => saveFile(name, data),
    load: async (name) => {
      const r = await fetch(`/__mb/save?name=${encodeURIComponent(name)}`);
      if (!r.ok) throw new Error(`mb.load: ${name}: ${r.status} (only \`mb serve\` has saved files)`);
      return r.text();
    },
    stats: (opts) => {
      const cpu = {};
      for (const [k, v] of Runtime.timings) {
        if (v.length) cpu[k] = { avg: v.reduce((a, b) => a + b, 0) / v.length, max: Math.max(...v) };
      }
      page2.stats.drawn = true;
      const top = JSON.parse(host_top_draws(TOP_STATS));
      return { ...JSON.parse(host_stats()), top, budgets: BUDGETS, cpu, ...page2.stats.report(opts) };
    },
    track: (node = 0) => host_track(node >>> 0),
    screenshot: async (name = `shot-${rt.brief().frames}`) => {
      present(host_redraw);
      const blob = await new Promise((ok) => canvas.toBlob(ok, "image/png"));
      if (!blob) throw new Error("mb.screenshot: the canvas gave no image");
      return saveFile(name.endsWith(".png") ? name : `${name}.png`, blob);
    }
  };
  if (!hasNative) {
    const q2 = new URLSearchParams(location.search);
    if (q2.has("speed")) window.mb.speed(Number(q2.get("speed")) || 1);
    if (q2.has("overlays")) drawOverlayLayer = drawOverlays(() => layout, screen2, m.sensors.includes("loudness"), m.replay_banner === "top");
    if (q2.has("stats")) drawStatsOverlay(page2.stats);
    if (q2.has("latency")) page2.latency.enabled = true;
  }
  if (boot.replay) {
    rt = await createSession(page2, { kind: "replay", log: boot.replay, loop: false });
    post({ op: "ready", info: JSON.parse(host_info()), bootMs: performance.now() - t0, paused: false });
    for (const f of boot.replay.frames) {
      await rt.session.assets.publishRecorded(f.assets ?? []);
      rt.step(f.dt, f.events ?? [], f, f.calls, f.lost);
    }
    post({ op: "replay-done", frames: boot.replay.frames.length, hash: host_draw_hash() });
    return;
  }
  const pin = !hasNative && !boot.web && !window.__MB_BOOT ? await pinnedRun(m.id) : null;
  const restorePin = pin?.mode === "restore" && dev ? pin : null;
  if (pin && !restorePin) {
    await restart({ replay: pin.log, loop: false, hold: true });
    post({ op: "ready", info: JSON.parse(host_info()), bootMs: performance.now() - t0, paused: true });
    const b = await window.mb.step(pin.at, { paced: false });
    console.info(`[mb] replayed ${b.frames} of the run's ${pin.log.frames.length} frames, held: mb.step(1) steps on, mb.resume() plays on`);
  } else {
    rt = await createSession(page2, { kind: "live" });
    const paused = boot.startPaused === true;
    post({ op: "ready", info: JSON.parse(host_info()), bootMs: performance.now() - t0, paused });
    void probe().then((p) => post({ op: "probe", probe: p }));
    if (restorePin) rt.startHeld();
    else if (paused) rt.startPaused();
    else rt.start();
  }
  dev?.attach({
    game: m.id,
    boot: boot.dev,
    save: () => {
      const snap = rt.snapshot();
      if (snap.frames === 0 || snap.log.game !== m.id) return null;
      return { v: 1, game: m.id, frames: snap.frames, time: snap.time, held: rt.held, log: snap.log, hashes: [...rt.frameHashes()] };
    },
    swapCode: (module2) => {
      page2.module = module2;
    },
    invalidate: (paths) => {
      if (paths === null) page2.assets.clear();
      else for (const p of paths) page2.assets.delete(p);
    },
    fresh: (held) => restart({ hold: held }),
    restore: restore2,
    frames: () => rt.brief().frames,
    replay: restorePin ? { v: 1, game: m.id, frames: restorePin.at, time: 0, held: true, log: { ...restorePin.log, frames: restorePin.log.frames.slice(0, restorePin.at) }, hashes: [] } : pin ? "played" : void 0
  });
}
async function pinnedRun(game) {
  const q2 = new URLSearchParams(location.search);
  const mode = q2.get("replay");
  if (mode !== "play" && mode !== "restore") return null;
  const r = await fetch("/__mb/replay", { cache: "no-store" });
  if (!r.ok) throw new Error(`?replay: /__mb/replay: ${r.status} (mb serve --replay serves the run)`);
  const log2 = await r.json();
  if (log2.game !== game) throw new Error(`?replay: the run is of ${log2.game}, not ${game}`);
  const at = Number(q2.get("at") ?? log2.frames.length);
  return { log: log2, at: Number.isInteger(at) && at >= 0 ? Math.min(at, log2.frames.length) : log2.frames.length, mode };
}
async function saveFile(name, data) {
  const body = data instanceof Uint8Array ? new Blob([data.slice()]) : data;
  const r = await fetch(`/__mb/save?name=${encodeURIComponent(name)}`, { method: "POST", body });
  const text = await r.text();
  if (!r.ok) throw new Error(`mb.save: ${text || r.status} (only \`mb serve\` saves files)`);
  return text;
}
main().catch((e) => {
  post({ op: "error", message: `boot failed: ${e instanceof Error ? e.stack ?? e.message : String(e)}` });
  dev?.bootFailed();
});
