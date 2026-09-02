plugins {
    kotlin("jvm") version "1.9.0"
}

group = "com.sdkwork"
version = "1.0.0"

base {
    archiveBaseName.set("sdkwork-webserver-app-sdk")
}

repositories {
    mavenCentral()
}

dependencies {
    implementation("com.sdkwork:sdk-common:1.0.0")
    implementation("com.squareup.okhttp3:okhttp:4.12.0")
    implementation("com.fasterxml.jackson.core:jackson-databind:2.16.0")
    implementation("com.fasterxml.jackson.module:jackson-module-kotlin:2.16.0")
    implementation(kotlin("stdlib"))

}

tasks.test {
    useJUnitPlatform()
}

kotlin {
    jvmToolchain(21)
}
