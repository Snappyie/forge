package io.forge.sdk;

public interface JobHandler {
    Object handle(JobContext ctx) throws Exception;
}
