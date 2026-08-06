//go:build helixdb_uniffi

package helix

import (
	"errors"
	"fmt"

	native "github.com/helixdb/helix-db/sdks/go/internal/uniffi/helixdb"
)

func openEmbedded(source HelixDbSource, reader bool, cache *EmbeddedCacheConfig) (nativeDB, error) {
	nativeSource, err := toNativeSource(source)
	if err != nil {
		return nil, err
	}
	if reader {
		if cache != nil {
			nativeCache, err := nativeCacheConfig(*cache)
			if err != nil {
				return nil, err
			}
			db, err := native.HelixDbOpenReaderWithConfig(nativeSource, nativeCache)
			if err != nil {
				return nil, err
			}
			return &uniffiDatabase{db: db}, nil
		}
		db, err := native.HelixDbOpenReader(nativeSource)
		if err != nil {
			return nil, err
		}
		return &uniffiDatabase{db: db}, nil
	}
	if cache != nil {
		nativeCache, err := nativeCacheConfig(*cache)
		if err != nil {
			return nil, err
		}
		db, err := native.HelixDbOpenWithConfig(nativeSource, nativeCache)
		if err != nil {
			return nil, err
		}
		return &uniffiDatabase{db: db}, nil
	}
	db, err := native.HelixDbOpen(nativeSource)
	if err != nil {
		return nil, err
	}
	return &uniffiDatabase{db: db}, nil
}

func nativeCacheConfig(cache EmbeddedCacheConfig) (native.EmbeddedCacheConfig, error) {
	var mode native.EmbeddedCacheMode
	switch value := cache.Mode.(type) {
	case VectorMemoryOnlyCache:
		mode = native.EmbeddedCacheModeVectorMemoryOnly{}
	case MemoryCache:
		mode = native.EmbeddedCacheModeMemory{}
	case HybridCache:
		mode = native.EmbeddedCacheModeHybrid{
			SlateMemoryBytes:     value.SlateMemoryBytes,
			SlateDiskPath:        value.SlateDiskPath,
			SlateDiskBytes:       value.SlateDiskBytes,
			ObjectStoreDiskPath:  value.ObjectStoreDiskPath,
			ObjectStoreDiskBytes: value.ObjectStoreDiskBytes,
		}
	default:
		return native.EmbeddedCacheConfig{}, fmt.Errorf("unsupported EmbeddedCacheMode %T", cache.Mode)
	}
	return native.EmbeddedCacheConfig{VectorMemoryBytes: cache.VectorMemoryBytes, Mode: mode}, nil
}

type uniffiDatabase struct{ db *native.HelixDb }

func (d *uniffiDatabase) QueryJson(request []byte) ([]byte, error) { return d.db.QueryJson(request) }
func (d *uniffiDatabase) Close() error                             { return d.db.Close() }
func (d *uniffiDatabase) Graph(request []byte, spec graphLoadSpec) (graphBackend, error) {
	value, err := d.db.Graph(request, nativeGraphSpec(spec))
	if err != nil {
		return nil, err
	}
	if value == nil {
		return nil, errors.New("helix: native graph loader returned nil")
	}
	return &uniffiGraph{graph: value}, nil
}

func toNativeSource(source HelixDbSource) (native.HelixDbSource, error) {
	switch source := source.(type) {
	case InMemorySource:
		return native.HelixDbSourceInMemory{Database: source.Database}, nil
	case DiskSource:
		return native.HelixDbSourceDisk{Root: source.Root, Database: source.Database}, nil
	case ObjectStorageSource:
		var endpoint *string
		if source.Endpoint != "" {
			endpoint = &source.Endpoint
		}
		return native.HelixDbSourceObjectStorage{
			Database:  source.Database,
			Bucket:    source.Bucket,
			Region:    source.Region,
			Endpoint:  endpoint,
			AllowHttp: source.AllowHTTP,
		}, nil
	default:
		return nil, fmt.Errorf("unsupported HelixDbSource %T", source)
	}
}
