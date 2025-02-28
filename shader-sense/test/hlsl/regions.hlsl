#define CONDITION_DEFINED 1

void main() {

#if CONDITION_DEFINED
    float a = 0;
#elif defined(CONDITION_DEFINED)
    float a = 3;
#else
    float a = 1;
#endif

#ifdef CONDITION_DEFINED
    float b = 1;
#else
    float b = 2;
#endif

#ifndef CONDITION_DEFINED
    float c = 1;
#else
    float c = 2;
#endif

#ifdef CONDITION_NOT_DEFINED
    float d = 1;
#endif

#if 0 
    float e = 1;
#endif

// paranthesized expression
#if (CONDITION_DEFINED && (CONDITION_NOT_DEFINED))
    float f = 1
#endif

// Binary expression
#if CONDITION_DEFINED && !CONDITION_DEFINED 
    float g = 1;
#endif

// unary expression
#if !CONDITION_DEFINED
    float f = 1;
#endif

// unary defined expression
#if !defined(CONDITION_DEFINED) && !defined(CONDITION_NOT_DEFINED)
    float h = 1.0;
#endif

// region depending on region
#if CONDITION_NOT_DEFINED
	#define CONDITION_NOT_DEFINED 1
#endif
#ifdef CONDITION_NOT_DEFINED
	#error "Should not be reached"
#endif
}