#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "s7.h"

static char *read_file(const char *path) {
  FILE *file = fopen(path, "rb");
  if (!file) {
    fprintf(stderr, "failed to open %s: %s\n", path, strerror(errno));
    return NULL;
  }

  if (fseek(file, 0, SEEK_END) != 0) {
    fprintf(stderr, "failed to seek %s\n", path);
    fclose(file);
    return NULL;
  }
  long size = ftell(file);
  if (size < 0) {
    fprintf(stderr, "failed to tell %s\n", path);
    fclose(file);
    return NULL;
  }
  rewind(file);

  char *buffer = (char *)calloc((size_t)size + 1, 1);
  if (!buffer) {
    fprintf(stderr, "out of memory reading %s\n", path);
    fclose(file);
    return NULL;
  }

  size_t read = fread(buffer, 1, (size_t)size, file);
  fclose(file);
  if (read != (size_t)size) {
    fprintf(stderr, "failed to read %s\n", path);
    free(buffer);
    return NULL;
  }
  buffer[size] = '\0';
  return buffer;
}

static char *wrap_program(const char *program) {
  const char *prefix = "(catch #t (lambda () (begin\n";
  const char *suffix = "\n)) (lambda args (list 'error args)))";
  size_t len = strlen(prefix) + strlen(program) + strlen(suffix) + 1;
  char *wrapped = (char *)malloc(len);
  if (!wrapped) return NULL;
  snprintf(wrapped, len, "%s%s%s", prefix, program, suffix);
  return wrapped;
}

int main(int argc, char **argv) {
  if (argc != 2) {
    fprintf(stderr, "usage: %s FILE.scm\n", argv[0]);
    return 2;
  }

  char *program = read_file(argv[1]);
  if (!program) return 1;
  char *wrapped = wrap_program(program);
  free(program);
  if (!wrapped) {
    fprintf(stderr, "out of memory wrapping program\n");
    return 1;
  }

  s7_scheme *sc = s7_init();
  s7_pointer result = s7_eval_c_string(sc, wrapped);
  free(wrapped);

  char *printed = s7_object_to_c_string(sc, result);
  if (printed) {
    puts(printed);
    free(printed);
  }

  s7_quit(sc);
  return 0;
}
