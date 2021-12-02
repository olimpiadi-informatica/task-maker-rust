#include <errno.h>
#include <malloc.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/mman.h>
#include <unistd.h>

#define handle_error(msg)   \
    do {                    \
        perror(msg);        \
        exit(EXIT_FAILURE); \
    } while (0)

char *buffer;

int main(int argc, char *argv[]) {
    char *p;
    int pagesize;

    pagesize = sysconf(_SC_PAGE_SIZE);
    if (pagesize == -1) handle_error("sysconf");

    /* Allocate a buffer aligned on a page boundary;
        initial protection is PROT_READ | PROT_WRITE */
    buffer = memalign(pagesize, 4 * pagesize);
    if (buffer == NULL) handle_error("memalign");
    if (mprotect(buffer + pagesize * 2, pagesize, PROT_READ) == -1)
        handle_error("mprotect");

    for (p = buffer; p < buffer + 1000 * pagesize;) *(p++) = 'a';
    exit(2);
}

