export type LocalText = readonly [en: string, zh: string];
export type LegalSection = {
  title: LocalText;
  paragraphs?: LocalText[];
  items?: LocalText[];
};
export const privacy: LegalSection[] = [
  {
    title: ["Quick summary", "快速了解"],
    items: [
      [
        "Editing and rendering happen on your computer. Media is sent to the local editing service, not uploaded to a remote editing server.",
        "剪辑和渲染在你的电脑上完成。素材交给本机剪辑服务处理，不会上传到远程剪辑服务器。",
      ],
      [
        "No account is required. The browser uses a local session to access your workspace.",
        "无需注册账号。浏览器通过本机会话访问你的工作区。",
      ],
      [
        "Projects and imported media are stored on your computer. This application does not collect analytics or sell your content.",
        "项目和已导入素材保存在你的电脑上。本应用不收集使用分析，也不出售你的内容。",
      ],
      [
        "Browser settings remember language, appearance and view preferences. Clearing browser data does not delete project files.",
        "浏览器保存语言、外观和视图偏好。清除浏览器数据不会删除项目文件。",
      ],
    ],
  },
  {
    title: ["How We Handle Your Content", "我们如何处理你的内容"],
    paragraphs: [
      [
        "All editing and processing happens locally on your device. The local service stores imported copies of your media, project history and generated artifacts. You control these files and the computer where they are stored.",
        "编辑和处理都在你的设备上进行。本机服务保存素材的导入副本、项目历史和生成的文件。你控制这些文件以及存放文件的电脑。",
      ],
      [
        "Local transcription uses a configured model on your computer. Model installation can require a download. Media is not sent to a remote transcription provider by this local workflow.",
        "本地转写使用电脑上配置的模型。安装模型可能需要下载文件；此本地流程不会将素材发送给远程转写服务。",
      ],
    ],
  },
  {
    title: ["Accounts & Authentication", "账号与身份验证"],
    paragraphs: [
      [
        "This local application has no cloud account, Google sign-in or profile collection. A local access credential protects the workspace; it is configured by the service and is not a user-facing connection setting.",
        "本地应用没有云账号、Google 登录或个人资料收集。本机访问凭据保护工作区，由服务配置，不需要用户在页面上设置连接。",
      ],
    ],
  },
  {
    title: ["Analytics", "使用分析"],
    paragraphs: [
      [
        "This application does not integrate the original website's analytics service and does not send visitor counts, editor interactions or media to an analytics provider.",
        "本应用未接入原版网站的分析服务，不会向分析服务发送访问统计、剪辑操作或素材。",
      ],
    ],
  },
  {
    title: ["Local Storage & Cookies", "本机存储与 Cookie"],
    items: [
      [
        "The local editing service stores projects, media, revision history and task results.",
        "本机剪辑服务保存项目、素材、修订历史和任务结果。",
      ],
      [
        "The browser remembers language, light or dark appearance, library layout and timeline views.",
        "浏览器记住语言、明暗外观、素材库布局和时间线视图。",
      ],
      [
        "A same-origin session cookie allows the page to use the local workspace. It is separate from project storage.",
        "同源会话 Cookie 允许页面使用本机工作区；它与项目存储分开。",
      ],
      [
        "Clearing browser data resets preferences and the session. Use the project library to delete a project. Deletion removes it from the library; immutable history, referenced media, task results and backups can remain on disk. Deletion is not secure erasure.",
        "清除浏览器数据会重置偏好与会话。删除项目请使用项目库。删除后项目不再出现在项目库中，但不可变历史、被引用素材、任务结果和备份可能仍保存在磁盘上。项目删除不等于安全擦除。",
      ],
    ],
  },
  {
    title: ["Third-Party Services", "第三方服务"],
    paragraphs: [
      [
        "The editor is served by the local service. Public blog and contributor pages request public content from Marble and GitHub when you open them. These providers receive the usual network request information, but no project or editing media is included. External links open the relevant provider's website.",
        "编辑器由本机服务提供。打开博客和贡献者页面时，页面会从 Marble 和 GitHub 请求公开内容。这些服务会收到常规网络请求信息，但请求不包含项目或剪辑素材。外部链接会打开对应服务的网站。",
      ],
    ],
  },
  {
    title: ["Your Rights", "你的权利"],
    items: [
      ["Use the editor without a cloud account.", "无需云账号即可使用编辑器。"],
      [
        "Control your local project files, exports and backups.",
        "管理你的本地项目文件、导出文件和备份。",
      ],
      [
        "Inspect the open source code and contact the community with privacy concerns.",
        "查看开源代码，并就隐私问题联系社区。",
      ],
    ],
  },
  {
    title: ["Open Source Transparency", "开源透明"],
    paragraphs: [
      [
        "OpenCut is open source. You can inspect how it handles data, modify the software and run it on your own computer.",
        "OpenCut 是开源软件。你可以检查它如何处理数据、修改软件，并在自己的电脑上运行。",
      ],
    ],
  },
  {
    title: ["Contact Us", "联系我们"],
    paragraphs: [
      [
        "For upstream OpenCut questions, open an issue in the OpenCut repository or contact oss@opencut.app. The local application's storage and processing details are described above.",
        "有关原版 OpenCut 的问题，可以在 OpenCut 仓库提交问题，或联系 oss@opencut.app。本地应用的存储和处理方式如上所述。",
      ],
    ],
  },
];
export const terms: LegalSection[] = [
  {
    title: ["Quick summary", "快速了解"],
    items: [
      [
        "You own your content. OpenCut makes no claim to ownership of your videos or projects.",
        "你拥有自己的内容。OpenCut 不主张对你的视频或项目拥有所有权。",
      ],
      [
        "Editing happens locally. The local service stores media and projects on your computer.",
        "编辑在本机完成。本机服务将素材和项目保存在你的电脑上。",
      ],
      [
        "Free for personal and commercial use, without an OpenCut watermark.",
        "可免费用于个人和商业用途，不添加 OpenCut 水印。",
      ],
      [
        "You are responsible for how you use the software and for complying with applicable law.",
        "你对软件的使用方式负责，并应遵守适用法律。",
      ],
      [
        "The software is provided as is, without warranties. Open source lets you inspect and modify it.",
        "软件按现状提供，不作担保。开源让你可以检查和修改软件。",
      ],
    ],
  },
  {
    title: ["Your Content, Your Rights", "你的内容与你的权利"],
    paragraphs: [
      [
        "You own everything you create. Editing and processing happen on your device. OpenCut makes no ownership or licensing claims over your videos, projects or other content.",
        "你拥有创作的内容。编辑和处理在你的设备上进行。OpenCut 不主张对你的视频、项目或其他内容拥有所有权或许可权。",
      ],
      [
        "You retain your intellectual property rights and can export and use your content as you choose. OpenCut imposes no watermark or content licensing restriction; rights in source material still apply.",
        "你保留知识产权，可以按自己的意愿导出和使用内容。OpenCut 不强加水印或内容许可限制；素材本身的权利仍然适用。",
      ],
    ],
  },
  {
    title: ["How You Can Use OpenCut", "如何使用 OpenCut"],
    items: [
      [
        "Create videos for personal, educational or commercial purposes.",
        "为个人、教育或商业用途制作视频。",
      ],
      [
        "Use the editor for client work and paid projects, and share the videos you create.",
        "为客户和付费项目使用编辑器，并分享制作的视频。",
      ],
      [
        "Modify and distribute the software under its MIT license. Respect the licenses of included dependencies and source material.",
        "按 MIT 许可证修改和分发软件，并遵守所含依赖和源素材的许可。",
      ],
      [
        "Do not use it for anything illegal in your jurisdiction.",
        "不得用于你所在司法辖区的违法活动。",
      ],
    ],
  },
  {
    title: ["AI Features", "AI 功能"],
    paragraphs: [
      [
        "Local transcription uses an installed model on your computer. The local workflow does not upload media to a remote provider. You can edit without using transcription.",
        "本地转写使用电脑上安装的模型。本地流程不会将素材上传给远程服务。你可以不使用转写功能进行剪辑。",
      ],
    ],
  },
  {
    title: ["Service", "服务"],
    paragraphs: [
      [
        "No cloud account is required. The software is provided as is, without warranties. Reliability and uninterrupted operation are not guaranteed.",
        "无需云账号。软件按现状提供，不作担保，也不保证可靠性或持续不中断运行。",
      ],
    ],
  },
  {
    title: ["Open Source Benefits", "开源的权利"],
    items: [
      [
        "Review the code to understand how your data is handled.",
        "检查代码，了解数据的处理方式。",
      ],
      [
        "Run the application yourself and modify it to suit your needs.",
        "自行运行应用，并按需要修改。",
      ],
      [
        "Contribute improvements to the community under the applicable license.",
        "按照适用许可证向社区贡献改进。",
      ],
    ],
  },
  {
    title: ["Limitations and Liability", "责任限制"],
    paragraphs: [
      [
        "To the extent permitted by applicable law, the software is provided without liability for loss of data, content or the way it is used. The MIT license contains the applicable warranty and liability terms.",
        "在适用法律允许的范围内，软件不对数据或内容丢失以及使用方式承担责任。MIT 许可证规定了适用的担保与责任条款。",
      ],
      [
        "Projects and media are stored in the local workspace. Clearing browser data does not remove them; deleting workspace files or losing the device can cause data loss. Keep backups of important projects, media and exports. The community cannot recover files that only exist on your computer.",
        "项目和素材保存在本机工作区。清除浏览器数据不会移除它们；删除工作区文件或丢失设备可能导致数据丢失。请备份重要项目、素材和导出文件。社区无法恢复仅存于你电脑上的文件。",
      ],
    ],
  },
  {
    title: ["Service Changes", "服务变更"],
    paragraphs: [
      [
        "Software and terms can change. Significant upstream changes are discussed through the open source repository. Review the terms accompanying the software you install. You may continue running an older copy subject to its license.",
        "软件和条款可能变更。原版的重要变更通过开源仓库讨论。请查看所安装软件随附的条款。在遵守许可的前提下，你可以继续运行旧副本。",
      ],
    ],
  },
  {
    title: ["Stopping Use", "停止使用"],
    paragraphs: [
      [
        "You can stop using the application at any time. Clearing browser data removes browser preferences and sessions. Manage local projects in the project library and retain any backups or exports you need before removing local workspace files.",
        "你可以随时停止使用应用。清除浏览器数据会移除浏览器偏好与会话。请在项目库中管理本机项目，并在移除本地工作区文件前保留需要的备份或导出文件。",
      ],
    ],
  },
  {
    title: ["Contact Us", "联系我们"],
    paragraphs: [
      [
        "For upstream questions, contact the OpenCut community through GitHub, oss@opencut.app or X. Applicable law in your jurisdiction governs these terms; the open source community prefers to resolve issues through discussion.",
        "有关原版的问题，请通过 GitHub、oss@opencut.app 或 X 联系 OpenCut 社区。这些条款受你所在司法辖区的适用法律约束；开源社区希望通过讨论解决问题。",
      ],
    ],
  },
];
