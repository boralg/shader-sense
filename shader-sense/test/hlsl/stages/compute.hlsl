// Example: Write thread ID to output buffer
RWStructuredBuffer<uint> outputBuffer : register(u0);

// Define thread group size
[numthreads(8, 8, 1)]
void CSMain(uint3 dispatchThreadID : SV_DispatchThreadID)
{
    uint index = dispatchThreadID.x + dispatchThreadID.y * 8;
    outputBuffer[index] = index;
}