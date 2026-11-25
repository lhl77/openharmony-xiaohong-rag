#include "m2_ffi.h"
#include <stdio.h>
#include <string.h>

static int check(const char *json) {
  return json != NULL && strlen(json) < 4096 && strstr(json, "\"ok\":true") != NULL;
}

static int check_error(const char *json, const char *code) {
  return json != NULL && strstr(json, "\"ok\":false") != NULL &&
         strstr(json, code) != NULL;
}

int main(void) {
  char *result = m2_init_json("{\"config\":{\"chunkTargetChars\":128,\"chunkOverlapChars\":8,\"embeddingDimension\":0,\"minimumChunkChars\":1}}");
  if (!check(result)) { if (result) m2_free_string(result); return 1; }
  m2_free_string(result);
  result = m2_add_documents_json("{\"documents\":[{\"documentId\":\"c-smoke\",\"revision\":1,\"title\":\"C\",\"course\":\"ABI\",\"text\":\"C ABI smoke test phrase\"}]}");
  if (!check(result)) { if (result) m2_free_string(result); return 2; }
  m2_free_string(result);
  result = m2_search_scoped_json("{\"request\":{\"query\":\"smoke\"},\"allowedDocumentIds\":[\"c-smoke\"]}");
  if (!check(result) || strstr(result, "c-smoke") == NULL ||
      strstr(result, "keywordRank") == NULL) { if (result) m2_free_string(result); return 3; }
  m2_free_string(result);
  result = m2_search_scoped_json("{\"request\":{\"query\":\"smoke\"},\"allowedDocumentIds\":[]}");
  if (!check(result) || strstr(result, "\"hits\":[]") == NULL) { if (result) m2_free_string(result); return 4; }
  m2_free_string(result);
  result = m2_federated_search_json("{\"query\":\"smoke\",\"topK\":5,\"rrfK\":60}");
  if (!check(result) || strstr(result, "\"sourceId\":\"local\"") == NULL) { if (result) m2_free_string(result); return 12; }
  m2_free_string(result);
  result = m2_resolve_json("{\"documentId\":\"c-smoke\",\"revision\":1,\"chunkId\":\"c-smoke:r1:c0001\"}");
  if (!check(result) || strstr(result, "C ABI smoke test phrase") == NULL) { if (result) m2_free_string(result); return 5; }
  m2_free_string(result);
  result = m2_resolve_scoped_json("{\"reference\":{\"documentId\":\"c-smoke\",\"revision\":1,\"chunkId\":\"c-smoke:r1:c0001\"},\"allowedDocumentIds\":[]}");
  if (!check(result) || strstr(result, "\"data\":null") == NULL) { if (result) m2_free_string(result); return 6; }
  m2_free_string(result);
  result = m2_style_profile_json("{\"allowedDocumentIds\":[\"c-smoke\"]}");
  if (!check(result) || strstr(result, "averageParagraphLength") == NULL || strstr(result, "smoke test phrase") != NULL) { if (result) m2_free_string(result); return 7; }
  m2_free_string(result);
  result = m2_remove_document_json("{\"documentId\":\"c-smoke\"}");
  if (!check(result)) { if (result) m2_free_string(result); return 8; }
  m2_free_string(result);
  result = m2_remove_document_json("{\"document_id\":\"c-smoke\"}");
  if (!check_error(result, "INVALID_JSON")) { if (result) m2_free_string(result); return 9; }
  m2_free_string(result);
  result = m2_search_json("\xff");
  if (!check_error(result, "INVALID_REQUEST")) { if (result) m2_free_string(result); return 10; }
  m2_free_string(result);
  result = m2_search_json(NULL);
  if (!check_error(result, "NULL_INPUT")) { if (result) m2_free_string(result); return 11; }
  m2_free_string(result);
  puts("C ABI smoke test passed");
  return 0;
}