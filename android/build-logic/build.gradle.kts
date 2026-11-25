plugins {
    `kotlin-dsl`
}

dependencies {
    testImplementation(platform(libs.junit.bom))
    testImplementation(libs.junit.jupiter)
    testRuntimeOnly(libs.junit.platform.launcher)
}

tasks.test {
    useJUnitPlatform()
}

gradlePlugin {
    plugins {
        create("designTokens") {
            id = "rtspviewer.design-tokens"
            implementationClass = "rtspviewer.buildlogic.DesignTokensPlugin"
        }
    }
}
