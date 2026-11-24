//! Local dual-channel retrieval (keyword + optional vector) with source citations.
mod chunk;
mod engine;
mod tokenize;
mod types;
pub use engine::{Embedder,NoopEmbedder,RagEngine};
pub use types::*;

use std::ffi::{CStr,CString,c_char};
use std::panic::{AssertUnwindSafe,catch_unwind};
use std::sync::{Arc,OnceLock,RwLock};
const MAX_INPUT_BYTES: usize = 32 * 1024 * 1024;
static ENGINE:OnceLock<RwLock<Option<RagEngine>>>=OnceLock::new();
fn slot()->&'static RwLock<Option<RagEngine>>{ENGINE.get_or_init(||RwLock::new(None))}

pub fn init_engine(config:IndexConfig)->RagResult<()> {let engine=RagEngine::new(config,Arc::new(NoopEmbedder))?;*slot().write().map_err(|_|RagError::PoisonedLock)?=Some(engine);Ok(())}
pub fn install_engine(engine:RagEngine)->RagResult<()> {*slot().write().map_err(|_|RagError::PoisonedLock)?=Some(engine);Ok(())}
pub fn add_documents(request_json:&str)->RagResult<IndexStats>{let req:AddDocumentsRequest=serde_json::from_str(request_json)?;slot().read().map_err(|_|RagError::PoisonedLock)?.as_ref().ok_or(RagError::InvalidRequest("engine is not initialized".into()))?.add_documents(req.documents)}
pub fn search(request_json:&str)->RagResult<SearchResponse>{let req:SearchRequest=serde_json::from_str(request_json)?;slot().read().map_err(|_|RagError::PoisonedLock)?.as_ref().ok_or(RagError::InvalidRequest("engine is not initialized".into()))?.search(req)}
pub fn remove_document(document_id:&str)->RagResult<bool>{slot().read().map_err(|_|RagError::PoisonedLock)?.as_ref().ok_or(RagError::InvalidRequest("engine is not initialized".into()))?.remove_document(document_id)}

fn envelope<T:serde::Serialize>(res:RagResult<T>)->String{match res{Ok(data)=>serde_json::json!({"ok":true,"data":data}).to_string(),Err(e)=>serde_json::json!({"ok":false,"error":{"code":e.code(),"message":e.to_string()}}).to_string()}}
unsafe fn read(ptr:*const c_char)->RagResult<String>{if ptr.is_null(){return Err(RagError::NullPointer)}let bytes=unsafe{CStr::from_ptr(ptr)}.to_bytes();if bytes.len()>MAX_INPUT_BYTES{return Err(RagError::InvalidRequest("input exceeds 32 MiB".into()))}let text=std::str::from_utf8(bytes).map_err(|e|RagError::InvalidRequest(format!("input is not UTF-8: {e}")))?;Ok(text.to_owned())}
fn into_c(s:String)->*mut c_char{CString::new(s).map(CString::into_raw).unwrap_or(std::ptr::null_mut())}
fn ffi<T:serde::Serialize>(input:*const c_char,operation:impl FnOnce(&str)->RagResult<T>)->*mut c_char{match catch_unwind(AssertUnwindSafe(||{let response=unsafe{read(input)}.and_then(|value|operation(&value));into_c(envelope(response))})){Ok(value)=>value,Err(_)=>into_c(envelope::<()>(Err(RagError::InvalidRequest("internal panic caught at FFI boundary".into()))))}}
#[derive(serde::Deserialize)]#[serde(rename_all="camelCase")]struct ScopedSearchRequest{request:SearchRequest,allowed_document_ids:Vec<String>}
#[derive(serde::Deserialize)]#[serde(rename_all="camelCase")]struct ScopedResolveRequest{reference:SourceReference,allowed_document_ids:Vec<String>}
#[derive(serde::Deserialize)]#[serde(rename_all="camelCase")]struct RemoveRequest{document_id:String}
#[unsafe(no_mangle)] pub unsafe extern "C" fn m2_init_json(input:*const c_char)->*mut c_char{ffi(input,|v|{let r:InitRequest=serde_json::from_str(v)?;init_engine(r.config)})}
#[unsafe(no_mangle)] pub unsafe extern "C" fn m2_add_documents_json(input:*const c_char)->*mut c_char{ffi(input,add_documents)}
#[unsafe(no_mangle)] pub unsafe extern "C" fn m2_search_json(input:*const c_char)->*mut c_char{ffi(input,search)}
#[unsafe(no_mangle)] pub unsafe extern "C" fn m2_search_scoped_json(input:*const c_char)->*mut c_char{ffi(input,|v|{let r:ScopedSearchRequest=serde_json::from_str(v)?;slot().read().map_err(|_|RagError::PoisonedLock)?.as_ref().ok_or(RagError::InvalidRequest("engine is not initialized".into()))?.search_scoped(r.request,&r.allowed_document_ids)})}
#[unsafe(no_mangle)] pub unsafe extern "C" fn m2_resolve_json(input:*const c_char)->*mut c_char{ffi(input,|v|{let r:SourceReference=serde_json::from_str(v)?;slot().read().map_err(|_|RagError::PoisonedLock)?.as_ref().ok_or(RagError::InvalidRequest("engine is not initialized".into()))?.resolve(&r)})}
#[unsafe(no_mangle)] pub unsafe extern "C" fn m2_resolve_scoped_json(input:*const c_char)->*mut c_char{ffi(input,|v|{let r:ScopedResolveRequest=serde_json::from_str(v)?;slot().read().map_err(|_|RagError::PoisonedLock)?.as_ref().ok_or(RagError::InvalidRequest("engine is not initialized".into()))?.resolve_scoped(&r.reference,&r.allowed_document_ids)})}
#[unsafe(no_mangle)] pub unsafe extern "C" fn m2_style_profile_json(input:*const c_char)->*mut c_char{ffi(input,|v|{let r:StyleRequest=serde_json::from_str(v)?;slot().read().map_err(|_|RagError::PoisonedLock)?.as_ref().ok_or(RagError::InvalidRequest("engine is not initialized".into()))?.style_profile(r)})}
#[unsafe(no_mangle)] pub unsafe extern "C" fn m2_remove_document_json(input:*const c_char)->*mut c_char{ffi(input,|v|{let r:RemoveRequest=serde_json::from_str(v)?;remove_document(&r.document_id)})}
#[unsafe(no_mangle)] pub unsafe extern "C" fn m2_free_string(p:*mut c_char){if !p.is_null(){drop(unsafe{CString::from_raw(p)});}}

#[cfg(test)]mod tests{use super::*;#[test]fn bad_json_has_stable_envelope(){let e=envelope::<()>(Err(RagError::InvalidRequest("bad".into())));assert!(e.contains("INVALID_REQUEST"));}#[test]fn ffi_null_and_invalid_utf8_return_json_envelopes(){let null=unsafe{m2_search_json(std::ptr::null())};assert!(!null.is_null());let text=unsafe{CStr::from_ptr(null)}.to_str().unwrap();assert!(text.contains("NULL_INPUT"));unsafe{m2_free_string(null)};let invalid=[0xff_u8,0];let result=unsafe{m2_search_json(invalid.as_ptr().cast())};assert!(!result.is_null());let text=unsafe{CStr::from_ptr(result)}.to_str().unwrap();assert!(text.contains("INVALID_REQUEST"));unsafe{m2_free_string(result)}}}
