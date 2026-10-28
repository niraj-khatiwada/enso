// The Rust renderer in src/lib.rs, linked as libenso_quicklook.a.
#pragma once

char *enso_quicklook_render(const char *source);
void enso_quicklook_free(char *html);
