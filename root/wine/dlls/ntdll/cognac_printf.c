/* CognacOS x64 NTDLL printf ABI bridge.
 * This file deliberately uses the Windows x64 C variadic ABI at the exported
 * boundary and keeps all formatting independent of libc imports.
 * String conversion is deterministic Windows-1252 <-> UTF-16 for the bootstrap;
 * exact per-process ACP/NLS tables remain future work.
 */
#include <stdarg.h>
#include <stddef.h>
#include <stdint.h>
#include <limits.h>

typedef uint16_t WCHAR;
typedef unsigned long long U64;
typedef long long I64;

typedef struct { void *buf; size_t cap, used; int overflow; int wide; } OUT;
static void put_unit(OUT *o, uint32_t c)
{
    if (o->used < o->cap && o->buf) {
        if (o->wide) ((WCHAR *)o->buf)[o->used] = (WCHAR)c;
        else ((char *)o->buf)[o->used] = (char)(c <= 255 ? c : '?');
    } else if (o->buf && o->used >= o->cap) o->overflow = 1;
    if (o->used < (size_t)INT_MAX) o->used++; else o->overflow = 1;
}
static void repeat(OUT *o, uint32_t c, size_t n) { while (n--) put_unit(o,c); }
static size_t slen_a(const char *s, size_t max) { size_t n=0; if (!s) return 6; while(n<max && s[n]) n++; return n; }
static size_t slen_w(const WCHAR *s, size_t max) { size_t n=0; if (!s) return 6; while(n<max && s[n]) n++; return n; }
static uint32_t cp1252_to_unicode(unsigned c)
{
    static const uint16_t ext[32]={0x20ac,0x0081,0x201a,0x0192,0x201e,0x2026,0x2020,0x2021,0x02c6,0x2030,0x0160,0x2039,0x0152,0x008d,0x017d,0x008f,0x0090,0x2018,0x2019,0x201c,0x201d,0x2022,0x2013,0x2014,0x02dc,0x2122,0x0161,0x203a,0x0153,0x009d,0x017e,0x0178};
    return c>=0x80 && c<0xa0 ? ext[c-0x80] : c;
}
static unsigned unicode_to_cp1252(uint32_t c)
{
    unsigned i;
    if (c<0x80 || (c>=0xa0 && c<=0xff)) return c;
    for(i=0x80;i<0xa0;i++) if(cp1252_to_unicode(i)==c) return i;
    return '?';
}
static void emit_string(OUT *o, const void *s, int source_wide, int dest_wide, int precision, int width, int left)
{
    size_t n=0,i; int nulls=!s;
    if (nulls) n=6;
    else if(source_wide) n=slen_w((const WCHAR *)s, precision<0?(size_t)-1:(size_t)precision);
    else n=slen_a((const char *)s, precision<0?(size_t)-1:(size_t)precision);
    if(precision>=0 && n>(size_t)precision)n=(size_t)precision;
    if(!left) repeat(o,' ',width>(int)n?(size_t)width-n:0);
    for(i=0;i<n;i++) {
        uint32_t c;
        if(nulls)c="(null)"[i];
        else c=source_wide?((const WCHAR *)s)[i]:(unsigned char)((const char *)s)[i];
        if(!dest_wide && source_wide)c=unicode_to_cp1252(c);
        else if(dest_wide && !source_wide)c=cp1252_to_unicode(c);
        put_unit(o,c);
    }
    if(left) repeat(o,' ',width>(int)n?(size_t)width-n:0);
}
static size_t utoa_rev(U64 v,unsigned base,int upper,char *b)
{
    const char *d=upper?"0123456789ABCDEF":"0123456789abcdef";size_t n=0;
    do { b[n++]=d[v%base];v/=base; } while(v);return n;
}
static void emit_number(OUT *o,U64 value,int negative,unsigned base,int upper,int pointer,
                        int sign,int alt,int zero,int left,int width,int precision)
{
    char tmp[66];size_t n=utoa_rev(value,base,upper,tmp),digits=n,i;char prefix[3];size_t pn=0;
    if(value==0 && precision==0 && !pointer)digits=0;
    if(negative)prefix[pn++]='-';else if(sign)prefix[pn++]=(char)sign;
    if(pointer){prefix[pn++]='0';prefix[pn++]=upper?'X':'x';}
    else if(alt&&value){if(base==16){prefix[pn++]='0';prefix[pn++]=upper?'X':'x';}else if(base==8 && (!digits||tmp[n-1]!='0'))prefix[pn++]='0';}
    if(precision>(int)digits)zero=0;
    {size_t zeros=precision>(int)digits?(size_t)precision-digits:0;
     size_t pad=(width>(int)(pn+zeros+digits))?(size_t)width-pn-zeros-digits:0;
     if(!left&&!zero)repeat(o,' ',pad);
     for(i=0;i<pn;i++)put_unit(o,(unsigned char)prefix[i]);
     if(!left&&zero)repeat(o,'0',pad);
     repeat(o,'0',zeros);
     for(i=0;i<digits;i++)put_unit(o,(unsigned char)tmp[n-1-i]);
     if(left)repeat(o,' ',pad);}
}
static int format_core(OUT *o,const void *fmt,va_list input,int wide)
{
    va_list ap;size_t p=0;va_copy(ap,input);
    while(wide?((const WCHAR *)fmt)[p]!=0:((const char *)fmt)[p]!=0) {
        uint32_t ch=wide?((const WCHAR *)fmt)[p]:((const unsigned char *)fmt)[p];
        if(ch!='%'){put_unit(o,ch);p++;continue;}p++;
        ch=wide?((const WCHAR *)fmt)[p]:((const unsigned char *)fmt)[p];
        if(ch=='%'){put_unit(o,'%');p++;continue;}
        int left=0,alt=0,zero=0,sign=0,width=0,prec=-1,lh=0,source_wide=-1;
        int loop=1;
        while(loop){ch=wide?((const WCHAR *)fmt)[p]:((const unsigned char *)fmt)[p];switch(ch){
          case '-':left=1;p++;break;case '#':alt=1;p++;break;case '0':zero=1;p++;break;case '+':sign='+';p++;break;case ' ':if(!sign)sign=' ';p++;break;default:loop=0;break;}}
        ch=wide?((const WCHAR *)fmt)[p]:((const unsigned char *)fmt)[p];
        if(ch=='*'){width=va_arg(ap,int);p++;if(width<0){left=1;width=width==INT_MIN?INT_MAX:-width;}}
        else while((ch=wide?((const WCHAR *)fmt)[p]:((const unsigned char *)fmt)[p])>='0'&&ch<='9'){if(width<INT_MAX/10)width=width*10+(int)(ch-'0');p++;}
        ch=wide?((const WCHAR *)fmt)[p]:((const unsigned char *)fmt)[p];
        if(ch=='.'){p++;prec=0;ch=wide?((const WCHAR *)fmt)[p]:((const unsigned char *)fmt)[p];if(ch=='*'){prec=va_arg(ap,int);p++;if(prec<0)prec=-1;}else while((ch=wide?((const WCHAR *)fmt)[p]:((const unsigned char *)fmt)[p])>='0'&&ch<='9'){if(prec<INT_MAX/10)prec=prec*10+(int)(ch-'0');p++;}}
        ch=wide?((const WCHAR *)fmt)[p]:((const unsigned char *)fmt)[p];
        while(ch=='h'||ch=='l'||ch=='w'||ch=='I'||ch=='z'||ch=='t'||ch=='j'||ch=='L'){if(ch=='h')lh=1;else if(ch=='l'||ch=='w')source_wide=1;else if(ch=='I'||ch=='z'||ch=='t'||ch=='j')lh=2;p++;ch=wide?((const WCHAR *)fmt)[p]:((const unsigned char *)fmt)[p];}
        if(ch==0) break;
        p++;
        if(ch=='s'||ch=='S'||ch=='c'||ch=='C'){
            int wantwide;
            if(ch=='s')wantwide=source_wide>=0?source_wide:wide;
            else if(ch=='S')wantwide=source_wide>=0?source_wide:!wide;
            else wantwide=(ch=='C')?!wide:wide;
            if(ch=='c'||ch=='C'){uint32_t c=(uint32_t)va_arg(ap,int);if(wantwide&&!o->wide)c=unicode_to_cp1252(c);else if(!wantwide&&o->wide)c=cp1252_to_unicode(c);repeat(o,' ',width>1&&!left?(size_t)width-1:0);put_unit(o,c);if(left)repeat(o,' ',width>1?(size_t)width-1:0);}
            else {const void *s=wantwide?(const void *)va_arg(ap,const WCHAR *):(const void *)va_arg(ap,const char *);emit_string(o,s,wantwide,o->wide,prec,width,left);}
        } else if(ch=='d'||ch=='i'||ch=='u'||ch=='o'||ch=='x'||ch=='X'||ch=='p'){
            U64 v;int neg=0;unsigned base=10;int ptr=ch=='p';
            if(ptr){v=(uintptr_t)va_arg(ap,void *);base=16;zero=1;if(prec<0)prec=(int)(sizeof(void*)*2);}
            else if(ch=='d'||ch=='i'){I64 sv=lh==2?va_arg(ap,I64):lh==1?(short)va_arg(ap,int):va_arg(ap,int);neg=sv<0;v=neg?(U64)(-(sv+1))+1:(U64)sv;}
            else {v=lh==2?va_arg(ap,U64):lh==1?(unsigned short)va_arg(ap,unsigned int):va_arg(ap,unsigned int);if(ch=='o')base=8;else if(ch=='x'||ch=='X')base=16;}
            emit_number(o,v,neg,base,ch=='X',ptr,sign,alt,zero,left,width,prec);
        } else if(ch=='n'){int *q=va_arg(ap,int *);if(q)*q=(int)o->used;}
        else if(ch=='f'||ch=='F'||ch=='e'||ch=='E'||ch=='g'||ch=='G'){
            /* Deterministic fixed-point bootstrap for f/F; scientific/general
             * conversions are deliberately not approximated as ABI-correct. */
            double d=va_arg(ap,double);if(ch=='f'||ch=='F'){
                U64 scale=1,whole,frac;int n=prec<0?6:prec;int k;
                if(n>9) n=9;
                for(k=0;k<n;k++) scale*=10;
                if(d<0){put_unit(o,'-');d=-d;}else if(sign)put_unit(o,sign);
                whole=(U64)d;frac=(U64)((d-(double)whole)*(double)scale+0.5);
                if(frac>=scale){whole++;frac-=scale;}
                {char b[66];size_t z=utoa_rev(whole,10,0,b);while(z)put_unit(o,b[--z]);}
                if(n||alt){put_unit(o,'.');for(k=n-1;k>=0;k--){U64 q=scale?frac/scale:0;put_unit(o,(uint32_t)('0'+q));frac=scale?frac%scale:frac;scale=scale?scale/10:0;}}
            }else {va_end(ap);return -1;}
        } else {put_unit(o,'%');put_unit(o,ch);}
    }
    va_end(ap);return o->overflow?-1:(int)o->used;
}
static int run_a(char *b,size_t cap,const char *f,va_list ap){OUT o={b,cap,0,0,0};int r=format_core(&o,f,ap,0);if(b&&cap&&o.used<cap)b[o.used]=0;return r;}
static int run_w(WCHAR *b,size_t cap,const WCHAR *f,va_list ap){OUT o={b,cap,0,0,1};int r=format_core(&o,f,ap,1);if(b&&cap&&o.used<cap)b[o.used]=0;return r;}
__declspec(dllexport) int _vsnprintf(char *s,size_t n,const char *f,va_list a){return run_a(s,n,f,a);}
__declspec(dllexport) int _vsnwprintf(WCHAR *s,size_t n,const WCHAR *f,va_list a){return run_w(s,n,f,a);}
__declspec(dllexport) int _vscprintf(const char *f,va_list a){return run_a(NULL,(size_t)INT_MAX,f,a);}
__declspec(dllexport) int _vscwprintf(const WCHAR *f,va_list a){return run_w(NULL,(size_t)INT_MAX,f,a);}
__declspec(dllexport) int _vsnprintf_s(char *s,size_t size,size_t len,const char *f,va_list a){size_t cap=size<len?size:len;int r=run_a(s,cap,f,a);if(s&&size){if(r<0||((size_t)r>=cap))s[0]=0;else s[r]=0;}return r<0?-1:((size_t)r>=size?-1:r);}
__declspec(dllexport) int _vsnwprintf_s(WCHAR *s,size_t size,size_t len,const WCHAR *f,va_list a){size_t cap=size<len?size:len;int r=run_w(s,cap,f,a);if(s&&size){if(r<0||((size_t)r>=cap))s[0]=0;else s[r]=0;}return r<0?-1:((size_t)r>=size?-1:r);}
__declspec(dllexport) int _snprintf(char *s,size_t n,const char *f,...){va_list a;int r;va_start(a,f);r=_vsnprintf(s,n,f,a);va_end(a);return r;}
__declspec(dllexport) int _snwprintf(WCHAR *s,size_t n,const WCHAR *f,...){va_list a;int r;va_start(a,f);r=_vsnwprintf(s,n,f,a);va_end(a);return r;}
__declspec(dllexport) int _snprintf_s(char *s,size_t z,size_t n,const char *f,...){va_list a;int r;va_start(a,f);r=_vsnprintf_s(s,z,n,f,a);va_end(a);return r;}
__declspec(dllexport) int _snwprintf_s(WCHAR *s,size_t z,size_t n,const WCHAR *f,...){va_list a;int r;va_start(a,f);r=_vsnwprintf_s(s,z,n,f,a);va_end(a);return r;}
__declspec(dllexport) int vsprintf(char *s,const char *f,va_list a){return run_a(s,(size_t)INT_MAX,f,a);}
__declspec(dllexport) int vsprintf_s(char *s,size_t z,const char *f,va_list a){return _vsnprintf_s(s,z,z,f,a);}
__declspec(dllexport) int _vswprintf(WCHAR *s,const WCHAR *f,va_list a){return run_w(s,(size_t)INT_MAX,f,a);}
__declspec(dllexport) int vswprintf_s(WCHAR *s,size_t z,const WCHAR *f,va_list a){return _vsnwprintf_s(s,z,z,f,a);}
__declspec(dllexport) int sprintf(char *s,const char *f,...){va_list a;int r;va_start(a,f);r=run_a(s,(size_t)INT_MAX,f,a);va_end(a);return r;}
__declspec(dllexport) int sprintf_s(char *s,size_t z,const char *f,...){va_list a;int r;va_start(a,f);r=vsprintf_s(s,z,f,a);va_end(a);return r;}
__declspec(dllexport) int swprintf(WCHAR *s,const WCHAR *f,...){va_list a;int r;va_start(a,f);r=run_w(s,(size_t)INT_MAX,f,a);va_end(a);return r;}
__declspec(dllexport) int swprintf_s(WCHAR *s,size_t z,const WCHAR *f,...){va_list a;int r;va_start(a,f);r=vswprintf_s(s,z,f,a);va_end(a);return r;}
