cmake_minimum_required(VERSION 3.16)
project(xiaohong_rag LANGUAGES CXX)

if(NOT DEFINED OHOS_ARCH OR NOT DEFINED RUST_TARGET)
  message(FATAL_ERROR "Set OHOS_ARCH and RUST_TARGET with the OpenHarmony toolchain")
endif()
set(RUST_ARCHIVE "${CMAKE_CURRENT_SOURCE_DIR}/target/${RUST_TARGET}/release/libxiaohong_rag.a")
add_custom_command(OUTPUT "${RUST_ARCHIVE}"
  COMMAND cargo build --locked --release --target "${RUST_TARGET}"
  WORKING_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}"
  DEPENDS Cargo.toml Cargo.lock build.rs
    src/lib.rs src/engine.rs src/chunk.rs src/tokenize.rs src/types.rs
  VERBATIM)
add_custom_target(xiaohong_rag_rust DEPENDS "${RUST_ARCHIVE}")
add_library(xiaohong_rag SHARED napi/napi_init.cpp)
add_dependencies(xiaohong_rag xiaohong_rag_rust)
target_include_directories(xiaohong_rag PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/include)
target_link_libraries(xiaohong_rag PRIVATE "${RUST_ARCHIVE}" ace_napi.z hilog_ndk.z c++_shared dl m)
target_compile_features(xiaohong_rag PRIVATE cxx_std_17)
target_compile_options(xiaohong_rag PRIVATE -fvisibility=hidden)
