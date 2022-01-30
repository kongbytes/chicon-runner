db.getCollection('functions').deleteMany({});
db.getCollection('functions').insertMany([
    {
        publicId: 'cea9424a-70a4-4c86-b995-d989c321bf34',
        name: 'Fetch Winston version',
        environment: {
            name: "shell",
            baseImage: "alpine:3.15",
            fileExtension: "sh",
            executor: "/bin/sh"
        },
        capabilities:  {
            network: true,
            filesystem: true,
        },
        content: "# Install the jq binary\n"+
                    "apk add jq\n"+
                    "\n"+
                    "# Find Winston release\n"+
                    "WINSTON_RELEASE=$(jq .dependencies.winston.version package-lock.json)\n"+
                    "\n"+
                    "# Write results\n"+
                    "echo \"has_package_lock=true\" > /result/data.toml\n"+
                    "echo \"winston_release=$WINSTON_RELEASE\" >> /result/data.toml"
    },
    {
        publicId: 'aded2e3e-eb68-486c-8caa-4b0596816947',
        name: 'Find NPM vulnerabilities',
        environment: {
            name: "nodejs",
            baseImage: "node:16-alpine3.15",
            fileExtension: "sh",
            executor: "/bin/sh"
        },
        capabilities:  {
            network: true,
            filesystem: true,
        },
        content: "# Install the jq binary\n"+
                    "apk add jq\n"+
                    "\n"+
                    "# Perform NPM audit\n"+
                    "RESULT=$(npm audit --json --package-lock-only --audit-level moderate --prod 2> /dev/null)\n"+
                    "HIGH_VULNS=$(echo $RESULT | jq '.metadata.vulnerabilities.high')\n"+
                    "echo \"high_vulns=$HIGH_VULNS\" > /result/data.toml\n"+
                    "\n"
    },
    {
        publicId: '757ad52c-02de-480d-96cc-739aff16e8f9',
        name: 'Compute repository size',
        environment: {
            name: "shell",
            baseImage: "alpine:3.15",
            fileExtension: "sh",
            executor: "/bin/sh"
        },
        capabilities:  {
            network: false,
            filesystem: false,
        },
        content: "# Compute directory size\n"+
                    "DIR_SIZE=$(du -sh /home | cut -f1 -d$'\t')\n"+
                    "\n"+
                    "# Write results\n"+
                    "echo \"repository_size=$DIR_SIZE\" > /result/data.toml\n"
    }
]);

db.getCollection('repositories').deleteMany({});
db.getCollection('repositories').insertMany([
    {
        publicId: '8737dbd9-134b-4384-8ee7-69a121aaa4a8',
        name: 'nestjs-template',
        url: 'https://github.com/Saluki/nestjs-template.git',
        // branch: 'master',
        // directory: '',
        tags: [
            'nodejs'
        ]
    },
    {
        publicId: '915c60aa-4d39-43b8-b5cc-27b6bdda9b25',
        name: 'joi-security',
        url: 'https://github.com/Saluki/joi-security.git',
        // branch: 'master',
        // directory: '',
        tags: [
            'nodejs'
        ]
    }
]);
