//! Search engine implementations

// Re-export core types
pub use digse_core::{
    Engine, EngineCategory, EngineMetadata,
    SearchQuery, SearchResult, ResultType, Error, Result,
};

pub mod acfun;
pub mod adobe_stock;
pub mod ahmia;
pub mod alpinelinux;
pub mod annas_archive;
pub mod ansa;
pub mod aol;
pub mod apkmirror;
pub mod apple_app_store;
pub mod apple_maps;
pub mod archlinux;
pub mod artic;
pub mod artstation;
pub mod arxiv;
pub mod astrophysics_data_system;
pub mod azure;
pub mod baidu;
pub mod bandcamp;
pub mod base;
pub mod bilibili;
pub mod bing;
pub mod bing_images;
pub mod bing_news;
pub mod bing_videos;
pub mod bitchute;
pub mod boardreader;
pub mod bpb;
pub mod brave;
pub mod braveapi;
pub mod bt4g;
pub mod btdigg;
pub mod cachy_os;
pub mod cara;
pub mod ccc_media;
pub mod chatnoir;
pub mod chefkoch;
pub mod chinaso;
pub mod cloudflareai;
pub mod command;
pub mod core;
pub mod crates;
pub mod crossref;
pub mod currency_convert;
pub mod dailymotion;
pub mod deepl;
pub mod deezer;
pub mod demo;
pub mod demo_offline;
pub mod demo_online;
pub mod destatis;
pub mod deviantart;
pub mod devicons;
pub mod dictzone;
pub mod digbt;
pub mod discourse;
pub mod doaj;
pub mod docker_hub;
pub mod dogpile;
pub mod doku;
pub mod duckduckgo;
pub mod duckduckgo_definitions;
pub mod duckduckgo_extra;
pub mod duckduckgo_weather;
pub mod duckduckgo_web;
pub mod duden;
pub mod dummy;
pub mod dummy_offline;
pub mod e_360search;
pub mod e_360search_videos;
pub mod e_500px;
pub mod e_9gag;
pub mod ebay;
pub mod elasticsearch;
pub mod emojipedia;
pub mod fdroid;
pub mod findthatmeme;
pub mod fireball;
pub mod flaticon;
pub mod flickr;
pub mod flickr_noapi;
pub mod freesound;
pub mod frinkiac;
pub mod fyyd;
pub mod geizhals;
pub mod genius;
pub mod gitea;
pub mod github;
pub mod github_code;
pub mod gitlab;
pub mod gmx;
pub mod goodreads;
pub mod google;
pub mod google_images;
pub mod google_news;
pub mod google_play;
pub mod google_scholar;
pub mod google_videos;
pub mod grokipedia;
pub mod hackernews;
pub mod heexy;
pub mod hex;
pub mod huggingface;
pub mod il_post;
pub mod imdb;
pub mod imgur;
pub mod ina;
pub mod invidious;
pub mod ipernity;
pub mod iqiyi;
pub mod jisho;
pub mod json_engine;
pub mod kagi;
pub mod kickass;
pub mod lemmy;
pub mod libretranslate;
pub mod lib_rs;
pub mod lingva;
pub mod loc;
pub mod lucide;
pub mod luxxle;
pub mod marginalia;
pub mod mariadb_server;
pub mod mastodon;
pub mod material_icons;
pub mod mediathekviewweb;
pub mod mediawiki;
pub mod meilisearch;
pub mod metacpan;
pub mod microsoft_learn;
pub mod mixcloud;
pub mod mojeek;
pub mod mongodb;
pub mod moviepilot;
pub mod mozhi;
pub mod mrs;
pub mod mwmbl;
pub mod mysql_server;
pub mod naver;
pub mod niconico;
pub mod npm;
pub mod nvd;
pub mod nyaa;
pub mod odysee;
pub mod ollama;
pub mod openalex;
pub mod openclipart;
pub mod openlibrary;
pub mod open_meteo;
pub mod opensemantic;
pub mod openstreetmap;
pub mod openverse;
pub mod pdbe;
pub mod peertube;
pub mod pexels;
pub mod photon;
pub mod pinterest;
pub mod piped;
pub mod piratebay;
pub mod pixabay;
pub mod pixiv;
pub mod pkg_go_dev;
pub mod podchaser;
pub mod postgresql;
pub mod presearch;
pub mod privacywall;
pub mod public_domain_image_archive;
pub mod pubmed;
pub mod pypi;
pub mod quark;
pub mod qwant;
pub mod radio_browser;
pub mod recoll;
pub mod reddit;
pub mod repology;
pub mod resulthunter;
pub mod reuters;
pub mod rottentomatoes;
pub mod rumble;
pub mod s1search;
pub mod scanr_structures;
pub mod meta_engine;
pub mod seekninja;
pub mod selfhst;
pub mod semantic_scholar;
pub mod senscritique;
pub mod sepiasearch;
pub mod seznam;
pub mod sogou;
pub mod sogou_images;
pub mod sogou_videos;
pub mod sogou_wechat;
pub mod solidtorrents;
pub mod solr;
pub mod soundcloud;
pub mod sourcehut;
pub mod spotify;
pub mod springer;
pub mod sqlite;
pub mod stackexchange;
pub mod stackoverflow;
pub mod startpage;
pub mod steam;
pub mod swisscows;
pub mod swisscows_news;
pub mod tagesschau;
pub mod tiger;
pub mod tineye;
pub mod tokyotoshokan;
pub mod tootfinder;
pub mod torznab;
pub mod translated;
pub mod tubearchivist;
pub mod unsplash;
pub mod uxwing;
pub mod valkey_server;
pub mod vimeo;
pub mod voidlinux;
pub mod vuhuv;
pub mod wallhaven;
pub mod wikicommons;
pub mod wikidata;
pub mod wikipedia;
pub mod wolframalpha_api;
pub mod wolframalpha_noapi;
pub mod wordnik;
pub mod wttr;
pub mod www1x;
pub mod x1337;
pub mod xpath;
pub mod yacy;
pub mod yandex;
pub mod yahoo;
pub mod yahoo_news;
pub mod yandex_music;
pub mod yep;
pub mod youtube_api;
pub mod youtube_noapi;
pub mod zlibrary;

/// Register and return all available engines.
pub fn all_engines() -> Vec<Box<dyn Engine>> {
    vec![
        Box::new(acfun::AcfunEngine::new()),
        Box::new(adobe_stock::AdobeStockEngine::new()),
        Box::new(ahmia::AhmiaEngine::new()),
        Box::new(alpinelinux::AlpineLinuxEngine::new()),
        Box::new(annas_archive::AnnasArchiveEngine::new()),
        Box::new(ansa::AnsaEngine::new()),
        Box::new(aol::AolEngine::new()),
        Box::new(apkmirror::ApkmirrorEngine::new()),
        Box::new(apple_app_store::AppleAppStoreEngine::new()),
        Box::new(apple_maps::AppleMapsEngine::new()),
        Box::new(archlinux::ArchlinuxEngine::new()),
        Box::new(artic::ArticEngine::new()),
        Box::new(artstation::ArtstationEngine::new()),
        Box::new(arxiv::ArxivEngine::new()),
        Box::new(astrophysics_data_system::AstrophysicsDataSystemEngine::new()),
        Box::new(azure::AzureEngine::new()),
        Box::new(baidu::BaiduEngine::new()),
        Box::new(bandcamp::BandcampEngine::new()),
        Box::new(base::BaseEngine::new()),
        Box::new(bilibili::BilibiliEngine::new()),
        Box::new(bing::BingEngine::new()),
        Box::new(bing_images::BingImagesEngine::new()),
        Box::new(bing_news::BingNewsEngine::new()),
        Box::new(bing_videos::BingVideosEngine::new()),
        Box::new(bitchute::BitchuteEngine::new()),
        Box::new(boardreader::BoardreaderEngine::new()),
        Box::new(bpb::BpbEngine::new()),
        Box::new(braveapi::BraveapiEngine::new()),
        Box::new(brave::BraveEngine::new()),
        Box::new(bt4g::Bt4gEngine::new()),
        Box::new(btdigg::BtdiggEngine::new()),
        Box::new(cachy_os::CachyOsEngine::new()),
        Box::new(cara::CaraEngine::new()),
        Box::new(ccc_media::CccMediaEngine::new()),
        Box::new(chatnoir::ChatnoirEngine::new()),
        Box::new(chefkoch::ChefkochEngine::new()),
        Box::new(chinaso::ChinasoEngine::new()),
        Box::new(cloudflareai::CloudflareaiEngine::new()),
        Box::new(command::CommandEngine::new()),
        Box::new(core::CoreEngine::new()),
        Box::new(crates::CratesEngine::new()),
        Box::new(crossref::CrossrefEngine::new()),
        Box::new(currency_convert::CurrencyConvertEngine::new()),
        Box::new(dailymotion::DailymotionEngine::new()),
        Box::new(deepl::DeeplEngine::new()),
        Box::new(deezer::DeezerEngine::new()),
        Box::new(demo::DemoEngine::new()),
        Box::new(demo_offline::DemoOfflineEngine::new()),
        Box::new(demo_online::DemoOnlineEngine::new()),
        Box::new(destatis::DestatisEngine::new()),
        Box::new(deviantart::DeviantartEngine::new()),
        Box::new(devicons::DeviconsEngine::new()),
        Box::new(dictzone::DictzoneEngine::new()),
        Box::new(digbt::DigbtEngine::new()),
        Box::new(discourse::DiscourseEngine::new()),
        Box::new(doaj::DoajEngine::new()),
        Box::new(docker_hub::DockerHubEngine::new()),
        Box::new(dogpile::DogpileEngine::new()),
        Box::new(doku::DokuEngine::new()),
        Box::new(duckduckgo_definitions::DuckDuckGoDefinitionsEngine::new()),
        Box::new(duckduckgo::DuckDuckGoEngine::new()),
        Box::new(duckduckgo_extra::DuckDuckGoExtraEngine::new()),
        Box::new(duckduckgo_weather::DuckDuckGoWeatherEngine::new()),
        Box::new(duckduckgo_web::DuckDuckGoWebEngine::new()),
        Box::new(duden::DudenEngine::new()),
        Box::new(dummy::DummyEngine::new()),
        Box::new(dummy_offline::DummyOfflineEngine::new()),
        Box::new(e_360search::Search360Engine::new()),
        Box::new(e_360search_videos::Search360VideosEngine::new()),
        Box::new(e_500px::Px500Engine::new()),
        Box::new(e_9gag::Gag9Engine::new()),
        Box::new(ebay::EBayEngine::new()),
        Box::new(elasticsearch::ElasticsearchEngine::new()),
        Box::new(emojipedia::EmojipediaEngine::new()),
        Box::new(fdroid::FDroidEngine::new()),
        Box::new(findthatmeme::FindThatMemeEngine::new()),
        Box::new(fireball::FireballEngine::new()),
        Box::new(flaticon::FlaticonEngine::new()),
        Box::new(flickr::FlickrEngine::new()),
        Box::new(flickr_noapi::FlickrNoApiEngine::new()),
        Box::new(freesound::FreesoundEngine::new()),
        Box::new(frinkiac::FrinkiacEngine::new()),
        Box::new(fyyd::FyydEngine::new()),
        Box::new(geizhals::GeizhalsEngine::new()),
        Box::new(genius::GeniusEngine::new()),
        Box::new(gitea::GiteaEngine::new()),
        Box::new(github_code::GitHubCodeEngine::new()),
        Box::new(github::GitHubEngine::new()),
        Box::new(gitlab::GitLabEngine::new()),
        Box::new(gmx::GmxEngine::new()),
        Box::new(goodreads::GoodreadsEngine::new()),
        Box::new(google::GoogleEngine::new()),
        Box::new(google_images::GoogleImagesEngine::new()),
        Box::new(google_news::GoogleNewsEngine::new()),
        Box::new(google_play::GooglePlayEngine::new()),
        Box::new(google_scholar::GoogleScholarEngine::new()),
        Box::new(google_videos::GoogleVideosEngine::new()),
        Box::new(grokipedia::GrokipediaEngine::new()),
        Box::new(hackernews::HackerNewsEngine::new()),
        Box::new(heexy::HeexyEngine::new()),
        Box::new(hex::HexEngine::new()),
        Box::new(huggingface::HuggingfaceEngine::new()),
        Box::new(il_post::IlPostEngine::new()),
        Box::new(imdb::ImdbEngine::new()),
        Box::new(imgur::ImgurEngine::new()),
        Box::new(ina::InaEngine::new()),
        Box::new(invidious::InvidiousEngine::new()),
        Box::new(ipernity::IpernityEngine::new()),
        Box::new(iqiyi::IqiyiEngine::new()),
        Box::new(jisho::JishoEngine::new()),
        Box::new(json_engine::JsonEngine::new()),
        Box::new(kagi::KagiEngine::new()),
        Box::new(kickass::KickassEngine::new()),
        Box::new(lemmy::LemmyEngine::new()),
        Box::new(libretranslate::LibretranslateEngine::new()),
        Box::new(lib_rs::LibRsEngine::new()),
        Box::new(lingva::LingvaEngine::new()),
        Box::new(loc::LocEngine::new()),
        Box::new(lucide::LucideEngine::new()),
        Box::new(luxxle::LuxxleEngine::new()),
        Box::new(marginalia::MarginaliaEngine::new()),
        Box::new(mariadb_server::MariadbServerEngine::new()),
        Box::new(mastodon::MastodonEngine::new()),
        Box::new(material_icons::MaterialIconsEngine::new()),
        Box::new(mediathekviewweb::MediathekviewwebEngine::new()),
        Box::new(mediawiki::MediaWikiEngine::new()),
        Box::new(meilisearch::MeilisearchEngine::new()),
        Box::new(metacpan::MetacpanEngine::new()),
        Box::new(microsoft_learn::MicrosoftLearnEngine::new()),
        Box::new(mixcloud::MixcloudEngine::new()),
        Box::new(mojeek::MojeekEngine::new_general()),
        Box::new(mongodb::MongoDbEngine::new()),
        Box::new(moviepilot::MoviepilotEngine::new()),
        Box::new(mozhi::MozhiEngine::new()),
        Box::new(mrs::MrsEngine::new()),
        Box::new(mwmbl::MwmblEngine::new()),
        Box::new(mysql_server::MysqlServerEngine::new()),
        Box::new(naver::NaverEngine::new()),
        Box::new(niconico::NiconicoEngine::new()),
        Box::new(npm::NpmEngine::new()),
        Box::new(nvd::NvdEngine::new()),
        Box::new(nyaa::NyaaEngine::new()),
        Box::new(odysee::OdyseeEngine::new()),
        Box::new(ollama::OllamaEngine::new()),
        Box::new(openalex::OpenAlexEngine::new()),
        Box::new(openclipart::OpenClipartEngine::new()),
        Box::new(openlibrary::OpenLibraryEngine::new()),
        Box::new(open_meteo::OpenMeteoEngine::new()),
        Box::new(opensemantic::OpenSemanticEngine::new()),
        Box::new(openstreetmap::OpenStreetMapEngine::new()),
        Box::new(openverse::OpenverseEngine::new()),
        Box::new(pdbe::PdbeEngine::new()),
        Box::new(peertube::PeertubeEngine::new()),
        Box::new(pexels::PexelsEngine::new()),
        Box::new(photon::PhotonEngine::new()),
        Box::new(pinterest::PinterestEngine::new()),
        Box::new(piped::PipedEngine::new()),
        Box::new(piratebay::PirateBayEngine::new()),
        Box::new(pixabay::PixabayEngine::new_general()),
        Box::new(pixiv::PixivEngine::new()),
        Box::new(pkg_go_dev::PkgGoDevEngine::new()),
        Box::new(podchaser::PodchaserEngine::new()),
        Box::new(postgresql::PostgreSqlEngine::new()),
        Box::new(presearch::PresearchEngine::new()),
        Box::new(privacywall::PrivacywallEngine::new()),
        Box::new(public_domain_image_archive::PublicDomainImageArchiveEngine::new()),
        Box::new(pubmed::PubMedEngine::new()),
        Box::new(pypi::PypiEngine::new()),
        Box::new(quark::QuarkEngine::new()),
        Box::new(qwant::QwantEngine::new()),
        Box::new(radio_browser::RadioBrowserEngine::new()),
        Box::new(recoll::RecollEngine::new()),
        Box::new(reddit::RedditEngine::new()),
        Box::new(repology::RepologyEngine::new()),
        Box::new(resulthunter::ResulthunterEngine::new()),
        Box::new(reuters::ReutersEngine::new()),
        Box::new(rottentomatoes::RottenTomatoesEngine::new()),
        Box::new(rumble::RumbleEngine::new()),
        Box::new(s1search::S1SearchEngine::new()),
        Box::new(scanr_structures::ScanrStructuresEngine::new()),
        Box::new(meta_engine::MetaSearchEngine::new()),
        Box::new(seekninja::SeekNinjaEngine::new()),
        Box::new(selfhst::SelfhstEngine::new()),
        Box::new(semantic_scholar::SemanticScholarEngine::new()),
        Box::new(senscritique::SensCritiqueEngine::new()),
        Box::new(sepiasearch::SepiaSearchEngine::new()),
        Box::new(seznam::SeznamEngine::new()),
        Box::new(sogou_images::SogouImagesEngine::new()),
        Box::new(sogou::SogouEngine::new()),
        Box::new(sogou_videos::SogouVideosEngine::new()),
        Box::new(sogou_wechat::SogouWechatEngine::new()),
        Box::new(solidtorrents::SolidTorrentsEngine::new()),
        Box::new(solr::SolrEngine::new()),
        Box::new(soundcloud::SoundCloudEngine::new()),
        Box::new(sourcehut::SourceHutEngine::new()),
        Box::new(spotify::SpotifyEngine::new()),
        Box::new(springer::SpringerEngine::new()),
        Box::new(sqlite::SqliteEngine::new()),
        Box::new(stackexchange::StackExchangeEngine::new()),
        Box::new(stackoverflow::StackOverflowEngine::new()),
        Box::new(startpage::StartpageEngine::new()),
        Box::new(steam::SteamEngine::new()),
        Box::new(swisscows_news::SwisscowsNewsEngine::new()),
        Box::new(swisscows::SwisscowsEngine::new()),
        Box::new(tagesschau::TagesschauEngine::new()),
        Box::new(tiger::TigerEngine::new()),
        Box::new(tineye::TinEyeEngine::new()),
        Box::new(tokyotoshokan::TokyoToshokanEngine::new()),
        Box::new(tootfinder::TootfinderEngine::new()),
        Box::new(torznab::TorznabEngine::new()),
        Box::new(translated::TranslatedEngine::new()),
        Box::new(tubearchivist::TubearchivistEngine::new()),
        Box::new(unsplash::UnsplashEngine::new_general()),
        Box::new(uxwing::UxwingEngine::new()),
        Box::new(valkey_server::ValkeyServerEngine::new()),
        Box::new(vimeo::VimeoEngine::new()),
        Box::new(voidlinux::VoidlinuxEngine::new()),
        Box::new(vuhuv::VuhuvEngine::new()),
        Box::new(wallhaven::WallhavenEngine::new()),
        Box::new(wikicommons::WikicommonsEngine::new()),
        Box::new(wikidata::WikidataEngine::new()),
        Box::new(wikipedia::WikipediaEngine::new_general()),
        Box::new(wolframalpha_api::WolframalphaApiEngine::new()),
        Box::new(wolframalpha_noapi::WolframalphaNoapiEngine::new()),
        Box::new(wordnik::WordnikEngine::new()),
        Box::new(wttr::WttrEngine::new()),
        Box::new(www1x::Www1xEngine::new()),
        Box::new(x1337::X1337Engine::new()),
        Box::new(xpath::XpathEngine::new()),
        Box::new(yacy::YacyEngine::new()),
        Box::new(yandex::YandexEngine::new()),
        Box::new(yahoo::YahooEngine::new()),
        Box::new(yahoo_news::YahooNewsEngine::new()),
        Box::new(yandex_music::YandexMusicEngine::new()),
        Box::new(yep::YepEngine::new()),
        Box::new(youtube_api::YoutubeApiEngine::new()),
        Box::new(youtube_noapi::YouTubeNoApiEngine::new()),
        Box::new(zlibrary::ZlibraryEngine::new()),
    ]
}

pub fn get_engine(name: &str) -> Option<Box<dyn Engine>> {
    all_engines()
        .into_iter()
        .find(|e| e.name() == name)
}

pub fn engines_by_category(category: EngineCategory) -> Vec<Box<dyn Engine>> {
    all_engines()
        .into_iter()
        .filter(|e| e.category() == category)
        .collect()
}
