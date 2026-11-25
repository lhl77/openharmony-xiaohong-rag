#pragma once
#ifdef __cplusplus
extern "C" {
#endif
char* m2_init_json(const char* json);
char* m2_add_documents_json(const char* json);
char* m2_search_json(const char* json);
char* m2_search_scoped_json(const char* json);
char* m2_federated_search_json(const char* json);
char* m2_resolve_json(const char* json);
char* m2_resolve_scoped_json(const char* json);
char* m2_style_profile_json(const char* json);
char* m2_remove_document_json(const char* json);
void m2_free_string(char* value);
#ifdef __cplusplus
}
#endif
