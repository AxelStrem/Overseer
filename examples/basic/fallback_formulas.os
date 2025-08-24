int v (fallback=10) = null

int a (fallback=$(b*c)) = null
int b (fallback=$(d)) = null
int c = 3
int d = 10

int a1 = $(b1*D/c1)
int b1 = 2
div D {
    int c1 = 10
}

