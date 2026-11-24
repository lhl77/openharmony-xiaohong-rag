import rag from 'libxiaohong_rag.so';

export async function initializeM2(): Promise<string> {
  return rag.init(JSON.stringify({ config: {
    chunkTargetChars: 1024, chunkOverlapChars: 96,
    embeddingDimension: 0, minimumChunkChars: 24
  } }));
}

export async function indexNotes(notes: Array<{
  id: string; revision: number; title: string; course: string; text: string
}>): Promise<string> {
  return rag.addDocuments(JSON.stringify({ documents: notes.map((note) => ({
    documentId: note.id, revision: note.revision, title: note.title,
    course: note.course, text: note.text
  })) }));
}

export async function retrieve(query: string, course?: string): Promise<string> {
  return rag.search(JSON.stringify({ query, courseFilter: course, topK: 5,
    rrfK: 60, vectorWeight: 1, keywordWeight: 1 }));
}

// This is a caller-supplied selection filter, not identity authorization.
export async function retrieveAllowed(query: string, allowedDocumentIds: string[]): Promise<string> {
  return rag.searchScoped(JSON.stringify({ request: { query, topK: 5 }, allowedDocumentIds }));
}

export async function resolveCitation(documentId: string, revision: number, chunkId: string): Promise<string> {
  return rag.resolve(JSON.stringify({ documentId, revision, chunkId }));
}

export async function resolveAllowed(documentId: string, revision: number, chunkId: string,
  allowedDocumentIds: string[]): Promise<string> {
  return rag.resolveScoped(JSON.stringify({
    reference: { documentId, revision, chunkId }, allowedDocumentIds
  }));
}

export async function readStructure(allowedDocumentIds: string[]): Promise<string> {
  return rag.styleProfile(JSON.stringify({ allowedDocumentIds }));
}

export async function removeNote(documentId: string): Promise<string> {
  return rag.removeDocument(JSON.stringify({ documentId }));
}import rag from 'libxiaohong_rag.so';

export async function initializeM2(): Promise<string> {
  return await rag.init(JSON.stringify({
    config: { chunkTargetChars: 1024, chunkOverlapChars: 96,
      embeddingDimension: 0, minimumChunkChars: 24 }
  }));
}

export async function indexNotes(note: {id:string;revision:number;title:string;course:string;text:string}[]): Promise<string> {
  return await rag.addDocuments(JSON.stringify({documents: note.map((x) => ({
    documentId:x.id, revision:x.revision, title:x.title, course:x.course, text:x.text
  }))}));
}

export async function retrieve(query:string, course?:string): Promise<string> {
  return await rag.search(JSON.stringify({query, courseFilter:course, topK:5,
    rrfK:60, vectorWeight:1, keywordWeight:1}));
}

export async function retrieveOnlyFromAllowedNotes(query:string, noteIds:string[]): Promise<string> {
  return await rag.searchScoped(JSON.stringify({request:{query, topK:5, rrfK:60,
	vectorWeight:1, keywordWeight:1}, allowedDocumentIds:noteIds}));
}

export async function resolveCitation(documentId:string, revision:number, chunkId:string): Promise<string> {
  return await rag.resolve(JSON.stringify({documentId, revision, chunkId}));
}

export async function resolveCitationInAllowedNotes(documentId:string, revision:number, chunkId:string, noteIds:string[]): Promise<string> {
  return await rag.resolveScoped(JSON.stringify({reference:{documentId, revision, chunkId}, allowedDocumentIds:noteIds}));
}

export async function readStyleWithoutReadingProse(noteIds:string[]): Promise<string> {
  return await rag.styleProfile(JSON.stringify({allowedDocumentIds:noteIds}));
}

export async function removeNote(documentId:string): Promise<string> {
  return await rag.removeDocument(JSON.stringify({documentId}));
}

// Treat returned citation text as evidence; do not pass an unreferenced answer as grounded.
