cmake_minimum_required(VERSION 3.20)
project(mcx NONE)

include(ExternalProject)

set(MCX_CARGO_TARGET_DIR "${CMAKE_BINARY_DIR}/cargo")
set(MCX_PROFILE release)

# Auto-detect architecture, mirroring scripts/crossgen.sh / toolchain.cmake
execute_process(COMMAND uname -m OUTPUT_VARIABLE MCX_ARCH OUTPUT_STRIP_TRAILING_WHITESPACE)
if(MCX_ARCH STREQUAL "x86_64")
  set(MCX_RUST_TARGET x86_64-unknown-linux-musl)
elseif(MCX_ARCH STREQUAL "aarch64")
  set(MCX_RUST_TARGET aarch64-unknown-linux-musl)
else()
  message(FATAL_ERROR "Unsupported architecture: ${MCX_ARCH}. Supported: x86_64, aarch64")
endif()

set(MCX_BINARY "${MCX_CARGO_TARGET_DIR}/${MCX_RUST_TARGET}/${MCX_PROFILE}/mcx")

ExternalProject_Add(mcx_build
  SOURCE_DIR "${CMAKE_CURRENT_SOURCE_DIR}"
  BUILD_IN_SOURCE TRUE
  CONFIGURE_COMMAND ""
  BUILD_COMMAND env CARGO_TARGET_DIR=${MCX_CARGO_TARGET_DIR}
    cargo build --profile ${MCX_PROFILE} --locked --target ${MCX_RUST_TARGET}
  BUILD_ALWAYS TRUE
  INSTALL_COMMAND ""
  BUILD_BYPRODUCTS "${MCX_BINARY}"
)

install(FILES "${MCX_BINARY}"
  DESTINATION "${CMAKE_INSTALL_PREFIX}/bin"
  PERMISSIONS OWNER_READ OWNER_WRITE OWNER_EXECUTE GROUP_READ GROUP_EXECUTE WORLD_READ WORLD_EXECUTE
)
install(FILES docs/mcx.1
  DESTINATION "${CMAKE_INSTALL_PREFIX}/share/man/man1"
)
