cmake_minimum_required(VERSION 3.20)
project(interop_lab C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_CXX_STANDARD 17)

add_executable(interop_app src/main.c src/helper.cpp)
set_source_files_properties(
  src/main.c
  PROPERTIES COMPILE_OPTIONS "-MMD;-MF;${CMAKE_BINARY_DIR}/cmake-main.d"
)
set_source_files_properties(
  src/helper.cpp
  PROPERTIES COMPILE_OPTIONS "-MMD;-MF;${CMAKE_BINARY_DIR}/cmake-helper.d"
)

file(WRITE "${CMAKE_BINARY_DIR}/response.rsp" "-DRESP_FROM_FILE=1\n")

add_custom_command(
  OUTPUT "${CMAKE_BINARY_DIR}/response.o"
  COMMAND "${CMAKE_C_COMPILER}" "@${CMAKE_BINARY_DIR}/response.rsp" -MMD -MF "${CMAKE_BINARY_DIR}/response.d" -c "${CMAKE_CURRENT_SOURCE_DIR}/src/response.c" -o "${CMAKE_BINARY_DIR}/response.o"
  DEPENDS "${CMAKE_CURRENT_SOURCE_DIR}/src/response.c" "${CMAKE_BINARY_DIR}/response.rsp"
  VERBATIM
)

add_custom_target(response_file DEPENDS "${CMAKE_BINARY_DIR}/response.o")
