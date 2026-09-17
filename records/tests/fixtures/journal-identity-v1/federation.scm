(define-class (federation)
  ;; Trusted-local federation transport and reciprocal relationship protocol.
  ;; Durable peer evidence is always stored through Ledger.
  (define-method (*init* self standard (config '()))
    (if (not (byte-vector? (cadr (assoc 'public-key config))))
        (error 'argument-error "Federation public key is required"))
    ((self '~field!) 'standard standard)
    ((self '~field!) 'config (expression->byte-vector config)))

  (define-method (config self (path '()))
    (if (equal? path '(interface-public-key))
        ((self '~config-get) '(public-key))
        ((self '~config-get) path)))

  (define-method (update-config! self changes)
    (if (not (and (list? changes) (assoc 'path changes)
                  (pair? (cadr (assoc 'path changes))) (assoc 'value changes)))
        (error 'argument-error "Malformed Federation config change: ~S" changes))
    ((self '~config-set!)
     (cadr (assoc 'path changes)) (cadr (assoc 'value changes))))

  (define-method (peers self) ((self '~config-get) '(peers)))

  (define-method (peer self alias)
    (if (not (symbol? alias))
        (error 'bridge-name-error "Peer alias must be a symbol: ~S" alias))
    ((self '~config-get) `(peers ,alias)))

  (define-method (bridge! self ledger alias endpoint signing-key)
    ;; Establish or resume one reciprocal relationship through the same method.
    (let ((operation-data
           (and (list? endpoint) (assoc 'value endpoint)
                (assoc 'continuation endpoint)
                (cadr (assoc 'continuation endpoint))))
          (endpoint
           (if (and (list? endpoint) (assoc 'value endpoint))
               (cadr (assoc 'value endpoint)) endpoint)))
    (if operation-data
        ((self '~resume!) ledger 'bridge! alias
         `((alias ,alias) (endpoint ,endpoint)) operation-data signing-key)
        (begin
    (if (not (and (symbol? alias) (list? endpoint)
                  (assoc 'interface endpoint) (assoc 'remote-name endpoint)
                  (string? (cadr (assoc 'interface endpoint)))
                  (symbol? (cadr (assoc 'remote-name endpoint)))))
        (error 'argument-error "Malformed bridge endpoint descriptor: ~S" endpoint))
    (let* ((interface (cadr (assoc 'interface endpoint)))
           (remote-name (cadr (assoc 'remote-name endpoint)))
           (existing ((self 'peer) alias))
           (retired ((self '~config-get) `(retired ,alias)))
           (known (if (null? existing) retired existing))
           (remote-index (let ((entry (and (list? known) (assoc 'remote-index known))))
                           (if entry (cadr entry) -1)))
           (head-index
            (if (not (null? existing)) -1
                (let ((entry (and (list? known) (assoc 'head-index known))))
                  (if entry (cadr entry) -1))))
           (ledger-checkpoint
            ((ledger 'peer-head) alias '((checkpoint #t))))
           (last-index ((self '~required)
                        ledger-checkpoint 'accepted-index))
           (response
            ((self '~remote)
             interface
             `((function synchronize!)
               (arguments
                ((name ,remote-name)
                 ,@(if ((self '~peer-match?) alias endpoint 'local)
                       '((establish-check? #t)) '())
                 (response ,((ledger 'signed-head)
                             `((index ,(if (integer? remote-index) remote-index -1))
                               (through ,(if (integer? head-index) head-index -1)))))
                 (info ,((ledger 'descriptor) -1))
                 (interface ,((self '~config-get) '(endpoint)))
                 (remote-name ,alias)
                 (known-index ,(if (integer? last-index) last-index -1))))))))
      `((continuation
         ,((self '~continuation)
           `((operation bridge!)
             (arguments ((alias ,alias) (endpoint ,endpoint)))
             (response ,response)
             (expected-ledger-peer
              ,((self '~ledger-peer-digest) ledger alias))
             (expected-federation-peer
              ,((self '~federation-peer-digest) alias))) signing-key))))))))

  (define-method (delete-bridge! self ledger alias)
    (let ((existing ((self 'peer) alias)))
      (if (not (null? existing))
          ((self '~config-set!) `(retired ,alias)
           ((self '~alist-set) existing 'head-index (- ((ledger 'size)) 1))))
    ((self '~config-set!) `(peers ,alias) '())
    ((ledger 'delete-peer-head!) alias)
    #t))

  (define-method (synchronize! self ledger request)
    ;; Acceptor-side reciprocal signed-head exchange.
    (if (not (and (list? request) (assoc 'name request)
                  (assoc 'response request) (assoc 'interface request)
                  (assoc 'remote-name request)))
        (error 'bridge-sync-error "Malformed synchronization request: ~S" request))
    (let* ((name (cadr (assoc 'name request)))
           (interface (cadr (assoc 'interface request)))
           (remote-name (cadr (assoc 'remote-name request)))
           (known-index (if (assoc 'known-index request)
                            (cadr (assoc 'known-index request)) -1))
           (info (and (assoc 'info request)
                      (cadr (assoc 'info request))))
           (ledger-checkpoint
            ((ledger 'peer-head) name '((checkpoint #t))))
           (verified
            ((self '~verify-peer-response)
             (cadr (assoc 'response request)) info interface ledger-checkpoint))
           (active?
            (and (assoc 'establish-check? request)
                 (eq? (cadr (assoc 'establish-check? request)) #t)
                 ((self '~peer-match?) name
                  `((interface ,interface) (remote-name ,remote-name)) 'remote)
                 (equal? ((self '~ref) ((self 'peer) name) '(identity-id))
                         ((self '~required) verified 'identity-id))
                 (= ((self '~required) verified 'index)
                    ((self '~required) ledger-checkpoint 'accepted-index))
                 (= known-index (- ((ledger 'size)) 1))))
           (result
            (if active?
                `((ok? #t)
                  (established? #t)
                  (accepted-index ,((self '~required) ledger-checkpoint 'accepted-index))
                  (head-index ,(- ((ledger 'size)) 1))
                  (response ,((ledger 'signed-head) known-index))
                  (info ,((ledger 'descriptor) -1))
                  (interface ,((self '~config-get) '(endpoint))))
                ((ledger 'store-peer-head!)
                 name
                 `((mode accept)
                   (response ,(cadr (assoc 'response request)))
                   (info ,info)
                   (interface ,interface)
                   (remote-name ,remote-name)
                   (known-index ,known-index)
                   (verified ,verified)
                   (checkpoint ,((self '~required) ledger-checkpoint 'checkpoint)))))))
      (if active? #t
          ((self '~peer-set!) name interface remote-name 'remote verified
           (cadr (assoc 'accepted-index result)) (cadr (assoc 'head-index result))))
      result))

  (define-method (bridge-synchronize! self ledger alias signing-key)
    ;; Initiator-side exchange or resume through the same method.
    (let ((operation-data
           (and (list? alias) (assoc 'value alias)
                (assoc 'continuation alias)
                (cadr (assoc 'continuation alias))))
          (alias
           (if (and (list? alias) (assoc 'value alias))
               (cadr (assoc 'value alias)) alias)))
    (if operation-data
        ((self '~resume!) ledger 'bridge-synchronize!
         alias `((alias ,alias)) operation-data signing-key)
        (let* ((peer ((self 'peer) alias))
           (interface ((self '~ref) peer '(interface)))
           (remote-name ((self '~ref) peer '(remote-name)))
           (remote-index ((self '~ref) peer '(remote-index)))
           (ledger-checkpoint
            ((ledger 'peer-head) alias '((checkpoint #t))))
           (last-index ((self '~required)
                        ledger-checkpoint 'accepted-index))
           (response
            ((self '~remote)
             interface
             `((function synchronize!)
               (arguments
                ((name ,remote-name)
                 (response ,((ledger 'signed-head)
                             (if (integer? remote-index) remote-index -1)))
                 (info ,((ledger 'descriptor) -1))
                 (interface ,((self '~config-get) '(endpoint)))
                 (remote-name ,alias)
                 (known-index ,(if (integer? last-index) last-index -1))))))))
      `((continuation
         ,((self '~continuation)
           `((operation bridge-synchronize!)
             (arguments ((alias ,alias)))
             (response ,response)
             (expected-ledger-peer
              ,((self '~ledger-peer-digest) ledger alias))
             (expected-federation-peer
              ,((self '~federation-peer-digest) alias))) signing-key)))))))

  (define-method (~resume! self ledger expected-operation alias expected-arguments operation-data signing-key)
    ;; Apply one authenticated continuation without external I/O.
    (if (not (and (list? operation-data)
                  (assoc 'domain operation-data)
                  (assoc 'operation operation-data)
                  (assoc 'arguments operation-data)
                  (assoc 'expected-ledger-peer operation-data)
                  (assoc 'expected-federation-peer operation-data)
                  (assoc 'signature operation-data)))
        (error 'federation-error
               "Malformed federation continuation: ~S" operation-data))
    (if (not (and
              (eq? (cadr (assoc 'domain operation-data))
                   'sync-web/federation-continuation/v1)
              (eq? (cadr (assoc 'operation operation-data)) expected-operation)
              (equal? (cadr (assoc 'arguments operation-data))
                      expected-arguments)
              ((self '~continuation-verify?) operation-data signing-key)))
        (error 'authentication-error
               "Federation continuation does not match the authenticated operation"))
    (let ((ledger-matches?
           (equal? (cadr (assoc 'expected-ledger-peer operation-data))
                   ((self '~ledger-peer-digest) ledger alias)))
          (federation-matches?
           (equal? (cadr (assoc 'expected-federation-peer operation-data))
                   ((self '~federation-peer-digest) alias)))
          (established?
           (and (eq? expected-operation 'bridge!) (assoc 'response operation-data)
                (let ((entry (assoc 'established? (cadr (assoc 'response operation-data)))))
                  (and entry (eq? (cadr entry) #t))))))
      (if (and (not established?) (not (and ledger-matches? federation-matches?)))
          (if (and (eq? expected-operation 'bridge!)
                   ((self '~crossed-bridge?) alias expected-arguments))
              (begin
                ((self '~config-set!) `(peers ,alias) '())
                ((ledger 'delete-peer-head!) alias)
                (set! operation-data '((operation bridge-race))))
              (error 'federation-conflict
                     "Peer relationship changed while operation was in flight: ~S ~S"
                     ledger-matches? federation-matches?))))
    (if (equal? operation-data '((operation bridge-race)))
        '((failure
           ((tag bridge-race-error)
            (message "Crossed reciprocal bridge initiation; retry from one journal"))))
    (case (cadr (assoc 'operation operation-data))
      ((bridge!)
       (let* ((endpoint ((self '~required)
                         (cadr (assoc 'arguments operation-data)) 'endpoint))
              (interface ((self '~required) endpoint 'interface))
              (remote-name ((self '~required) endpoint 'remote-name))
              (response ((self '~required) operation-data 'response))
              (info ((self '~required) response 'info))
              (checkpoint
               ((ledger 'peer-head) alias '((checkpoint #t))))
              (verified
               ((self '~verify-peer-response)
                ((self '~required) response 'response)
                info interface checkpoint)))
         (if (and (assoc 'established? response)
                  (eq? (cadr (assoc 'established? response)) #t))
             (if ((self '~peer-match?) alias endpoint 'local) #t
                 (error 'federation-conflict
                        "Active peer changed during establish check: ~S" alias))
             (begin
               ((ledger 'store-peer-head!)
                alias
                `((mode establish)
                  (response ,((self '~required) response 'response))
                  (interface ,interface)
                  (remote-name ,remote-name)
                  (info ,info)
                  (verified ,verified)
                  (initiation local)
                  (checkpoint
                   ,(cadr (assoc 'expected-ledger-peer operation-data)))))
               ((self '~peer-set!) alias interface remote-name 'local verified
                ((self '~required) response 'accepted-index)
                ((self '~required) response 'head-index))
               ((self '~config-set!) `(retired ,alias) '())
               #t))))
      ((bridge-synchronize!)
       (let* ((response ((self '~required) operation-data 'response))
              (peer ((self 'peer) alias))
              (checkpoint
               ((ledger 'peer-head) alias '((checkpoint #t))))
              (verified
               ((self '~verify-peer-response)
                ((self '~required) response 'response)
                ((self '~required) response 'info)
                ((self '~ref) peer '(interface)) checkpoint)))
         ((self '~peer-set!) alias
          ((self '~ref) peer '(interface)) ((self '~ref) peer '(remote-name))
          ((self '~ref) peer '(initiation)) verified
          ((self '~required) response 'accepted-index)
          ((self '~required) response 'head-index))
         ((ledger 'store-peer-head!)
          alias `((mode update)
                  (response ,((self '~required) response 'response))
                  (interface ,((self '~ref) peer '(interface)))
                  (remote-name ,((self '~ref) peer '(remote-name)))
                  (verified ,verified)
                  (checkpoint
                   ,(cadr (assoc 'expected-ledger-peer operation-data)))))
         #t))
      (else
       (error 'federation-error
              "Unknown federation continuation operation: ~S"
              (cadr (assoc 'operation operation-data)))))))

  (define-method (~peer-set! self alias interface remote-name initiation
                             verified remote-index head-index)
    (let ((existing ((self 'peer) alias)))
      ((self '~config-set!) `(peers ,alias)
       `((interface ,interface)
         (remote-name ,remote-name)
         (initiation ,(if (null? existing) initiation
                          ((self '~ref) existing '(initiation))))
         (enabled? #t)
         (identity ,((self '~required) verified 'identity))
         (identity-id ,((self '~required) verified 'identity-id))
         (public-key ,((self '~required) verified 'public-key))
         (last-index ,((self '~required) verified 'index))
         (remote-index ,remote-index)
         (head-index ,head-index)))))

  (define-method (~ledger-peer-digest self ledger alias)
    ((self '~required)
     ((ledger 'peer-head) alias '((checkpoint #t))) 'checkpoint))

  (define-method (~federation-peer-digest self alias)
    (let ((peer ((self 'peer) alias)))
      (sync-hash
       (expression->byte-vector
        (map (lambda (key) (list key ((self '~ref) peer (list key))))
             (if (null? peer) '() '(interface remote-name initiation enabled?)))))))

  (define-method (~peer-match? self alias endpoint initiation)
    (let ((peer ((self 'peer) alias)))
      (and (not (null? peer))
           ((self '~ref) peer '(enabled?))
           (eq? ((self '~ref) peer '(initiation)) initiation)
           (equal? ((self '~ref) peer '(interface))
                   ((self '~ref) endpoint '(interface)))
           (equal? ((self '~ref) peer '(remote-name))
                   ((self '~ref) endpoint '(remote-name))))))

  (define-method (~crossed-bridge? self alias expected-arguments)
    ((self '~peer-match?) alias (cadr (assoc 'endpoint expected-arguments)) 'remote))

  (define-method (~continuation self body signing-key)
    (set! body (cons '(domain sync-web/federation-continuation/v1) body))
    (if (not (and (pair? signing-key)
                  (byte-vector? (car signing-key))
                  (byte-vector? (cdr signing-key))))
        (error 'authentication-error
               "Federation has no transient continuation signing key"))
    (append body
            `((signature
               ,(crypto-sign
                 (cdr signing-key) (expression->byte-vector body))))))

  (define-method (~continuation-verify? self operation-data signing-key)
    (let ((body ((self '~alist-set) operation-data 'signature #f #t))
          (signature (cadr (assoc 'signature operation-data))))
      (and (pair? signing-key) (byte-vector? (car signing-key))
           (byte-vector? signature)
           (crypto-verify
            (car signing-key) signature (expression->byte-vector body)))))

  (define-method (step! self ledger)
    ;; Return the inert internal operation plan selected by Federation policy.
    `((operations
       ,(let loop ((peers ((self 'peers))) (out '()))
          (if (null? peers) (reverse out)
              (let ((peer (cadar peers)))
                (loop
                 (cdr peers)
                 (if (and ((self '~ref) peer '(enabled?))
                          (eq? ((self '~ref) peer '(initiation)) 'local))
                     (cons `(bridge-synchronize! ,(caar peers)) out)
                     out))))))))

  (define-method (route self ledger request)
    ;; Follow exact routes or hydrate one local proof path.
    (if (and (assoc 'operation request)
             (eq? (cadr (assoc 'operation request)) 'hydrate))
        ((self '~trace-object)
         ledger
         (if (assoc 'index request) (cadr (assoc 'index request)) -1)
         ((self '~required) request 'path))
    (let* ((standard (sync-eval ((self '~field!) 'standard)))
           (route-source ((self '~required) request 'route-source))
           (route-target ((self '~required) request 'route-target))
           (index (if (assoc 'index request) (cadr (assoc 'index request)) -1))
           (history-indexes (and (assoc 'history-indexes request)
                                  (cadr (assoc 'history-indexes request))))
           (history-head-index (and (assoc 'history-head-index request)
                                     (cadr (assoc 'history-head-index request))))
           (roots (if (assoc 'roots request) (cadr (assoc 'roots request)) '())))
      (if (not (and (list? route-source) (list? route-target) (list? roots)
                    (or (not history-indexes)
                        (and (list? history-indexes)
                             (= (length history-indexes) (+ (length route-target) 1))
                             (let loop ((indexes history-indexes))
                               (or (null? indexes)
                                   (and (integer? (car indexes))
                                        (loop (cdr indexes)))))))))
          (error 'route-error "Invalid federation route/history shape: ~S" request))
      (set! roots
            (cons (cadr (assoc 'id
                               ((self '~ref) ((ledger 'config))
                                '(public identity))))
                  roots))
      (if (pair? route-target)
          (let* ((next (car route-target))
                 (edge ((ledger 'peer-head) next index))
                 (history-edge
                  (and history-indexes
                       ((ledger 'peer-head)
                        next `((local ,(car history-indexes))
                               (remote ,(cadr history-indexes))))))
                 (interface (cadr (assoc 'interface edge)))
                 (remote-name (cadr (assoc 'remote-name edge))))
            (sync-remote
             interface
             `((function route)
               (arguments
                ((route-source ,(append route-source (list remote-name)))
                 (route-target ,(cdr route-target))
                 (index ,(cadr (assoc 'index edge)))
                 ,@(if history-indexes
                       `((history-indexes
                          ,(cons (cadr (assoc 'index history-edge))
                                 (cddr history-indexes)))
                         (history-head-index
                          ,(cadr (assoc 'head-index history-edge))))
                       '())
                 (roots ,roots))))))
          (let* ((index (if (< index 0) (- ((ledger 'size)) 1) index))
                 (key-path ((self '~bridge-route-path)
                            (reverse route-source)
                            '(*crypto* interface public-key)))
                 (key-object
                  ((self '~trace-object) ledger index key-path))
                 (key-value ((self '~get) standard key-object key-path))
                 (paths (list '(*crypto* interface public-key)
                              '(*crypto* interface endpoint)
                              '(*crypto* journal identity id)))
                 (trace-paths
                  (map (lambda (path) ((self '~ledger-path) path))
                       (cons key-path paths)))
                 (object
                  (let loop ((paths paths) (proof key-object))
                    (if (null? paths) proof
                        (let* ((serialization
                                ((ledger 'trace) (cons index (car paths))))
                               (slice ((standard 'deserialize) serialization)))
                          (loop (cdr paths)
                                ((standard 'deep-merge!) slice proof))))))
                 (history-index (and history-indexes (car history-indexes)))
                 (history-object
                  (and history-index
                       ((ledger 'trace)
                        (list history-head-index history-index)))))
            `((route-source ,route-source)
              (terminal-index ,index)
              (roots ,(reverse roots))
              (object
               ,((standard 'serialize) object
                 (and (byte-vector? key-value)
                      `(lambda (node)
                         (letrec
                             ((deep-get
                               (lambda (node path)
                                 (if (null? path) node
                                     (let ((child
                                            (((sync-eval node) 'get)
                                             (car path))))
                                       (if (not (sync-node? child)) child
                                           (deep-get child (cdr path))))))))
                           (for-each (lambda (path) (deep-get node path))
                                     ',trace-paths))))))
              ,@(if history-object
                    `((history-index ,history-index)
                      (history-head-index ,history-head-index)
                      (history-object ,history-object))
                    '())))))))

  (define-method (invoke self ledger operation arguments route history identity signing-key
                         (indexed-proof? #f))
    ;; Construct, sign, deliver, and verify one federated application request.
    (if (not (memq operation '(get set! get-batch set-batch! resolve)))
        (error 'api-error "Function is not available through federation: ~S" operation))
    (if (not (and (symbol? identity) (list? route) (pair? route)))
        (error 'authentication-error "Invalid originating federation invocation"))
    (let* ((standard (sync-eval ((self '~field!) 'standard)))
           (origin-index (- ((ledger 'size)) 1))
           (route-response
            ((self 'route) ledger
             `((route-source ()) (route-target ,route) (index ,origin-index)
               ,@(if history `((history-indexes ,history)) '()))))
           (source-route (cadr (assoc 'route-source route-response)))
           (terminal-index (cadr (assoc 'terminal-index route-response)))
           (serialization (cadr (assoc 'object route-response)))
           (source-object ((standard 'deserialize) serialization))
           (proof-requested?
            (and (assoc 'proof? arguments)
                 (cadr (assoc 'proof? arguments))))
           (target-path ((self '~bridge-route-path) route '()))
           (target-object
            ((self '~trace-object) ledger origin-index target-path))
           (terminal-object ((self '~get) standard target-object target-path))
           (combined
            (if (and (sync-node? terminal-object)
                     (equal? (sync-digest terminal-object)
                             (sync-digest source-object)))
                ((standard 'deep-merge!) source-object terminal-object)
                (error 'integrity-error
                       "Terminal object is not contained in the origin's committed route")))
           (history-index
            (and history (cadr (assoc 'history-index route-response))))
           (history-head-index
            (and history
                 (cadr (assoc 'history-head-index route-response))))
           (history-origin-index
            (and history
                 (let ((selected (car history)))
                   (if (< selected 0)
                       (+ ((ledger 'size)) selected) selected))))
           (history-path
            (and history ((self '~bridge-route-path) route '() (cdr history))))
           (prepared-history-chain
            (and history
                 ((self '~get) standard target-object
                  (cons history-origin-index history-path))))
           (history-origin-head
            (and history
                 (if (or proof-requested?
                         (equal? prepared-history-chain '(unknown)))
                     ((self '~trace-object)
                      ledger history-origin-index history-path)
                     target-object)))
           (history-chain
            (and history
                 (if (or proof-requested?
                         (equal? prepared-history-chain '(unknown)))
                     ((self '~get) standard history-origin-head history-path)
                     prepared-history-chain)))
           (history-source
            (and history
                 ((standard 'deserialize)
                  (cadr (assoc 'history-object route-response)))))
           (history-merged
            (and history
                 (if (equal? (sync-digest history-chain)
                             (sync-digest history-source))
                     ((standard 'deep-merge!)
                      history-source history-chain)
                     (error 'integrity-error
                            "Ledger history path diverges from the working route"))))
           (data-head (if history history-merged combined))
           (data-head-index
            (if history history-head-index terminal-index))
           (data-index (if history history-index terminal-index))
           (route-roots (cadr (assoc 'roots route-response)))
           (self-route?
            (and (pair? route-roots) (pair? (cdr route-roots))
                 (equal? (car route-roots)
                         (car (reverse route-roots)))))
           (source-key-path
            (if self-route?
                '(*crypto* interface public-key)
                ((self '~bridge-route-path) (reverse source-route)
                 '(*crypto* interface public-key))))
           (source-key ((self '~get) standard combined source-key-path))
           (_
            (if (byte-vector? source-key) #t
                (error 'bridge-error
                       "Federation route is not ready: reverse interface key is unavailable")))
           (endpoint ((self '~get) standard combined '(*crypto* interface endpoint)))
           (audience ((self '~get) standard combined '(*crypto* journal identity id)))
           (private-key
            (and (pair? signing-key)
                 (equal? source-key (car signing-key))
                 (cdr signing-key)))
           (resolve? (eq? operation 'resolve))
           (original-proof? proof-requested?)
           (original-pinned?
            (and (assoc 'pinned? arguments)
                 (cadr (assoc 'pinned? arguments))))
           (wire-arguments
            (if resolve?
                (let* ((path (cadr (assoc 'path arguments)))
                       (path
                        (if (not history) path
                            (cond
                             ((and (pair? path) (integer? (car path))
                                   (< (car path) 0))
                              (cons data-index (cdr path)))
                             ((and (pair? path) (integer? (car path)))
                              (if (> (car path) data-index)
                                  (error 'integrity-error
                                         "Requested index is newer than the Ledger history path")
                                  path))
                             (else (cons data-index path))))))
                  ((self '~alist-set)
                   ((self '~alist-set)
                    ((self '~alist-set) arguments 'path path)
                    'proof? #t)
                   'pinned? #f))
                arguments))
           (unsigned `((identity ,identity)
                       (route-source ,source-route)
                       (route-target ())
                       (object ,serialization)
                       (terminal-index ,terminal-index)
                       (self-route? ,self-route?)
                       ,@(if history
                             `((data-object
                                ,((standard 'serialize) data-head))
                               (data-head-index ,data-head-index))
                             '())
                       (audience ,audience)))
           (signature
            (crypto-sign
             (if private-key private-key
                 (error 'authentication-error
                        "Transient interface key does not match terminal source key"))
             (expression->byte-vector
              ((self '~invocation-message) operation wire-arguments unsigned))))
           (response
            ((self '~remote)
             endpoint
             `((function ,operation)
               (arguments ,wire-arguments)
               (invocation (,@unsigned (signature ,signature)))))))
      ;; Resolve responses carry proof and are re-anchored by the origin.
      (if (not resolve?) response
          (let* ((verified-response
                  ((self '~verify-resolve-response)
                   ledger data-head wire-arguments response))
                 (content (cadr (assoc 'content verified-response)))
                 (terminal-proof
                  ((standard 'deserialize)
                   (cadr (assoc 'proof verified-response))))
                 (origin-object-path
                  (if history
                      (cons history-origin-index history-path)
                      target-path))
                 (origin-base
                  (and original-proof?
                       ((standard 'deep-slice!)
                        (if history history-origin-head target-object)
                        ((self '~ledger-path) origin-object-path))))
                 (origin-proof
                  (and original-proof?
                       ((standard 'deep-set!)
                        origin-base ((self '~ledger-path) origin-object-path)
                        terminal-proof)))
                 (wire-path (cadr (assoc 'path wire-arguments)))
                 (origin-value-path
                  (append
                   origin-object-path
                   (if history (list data-index) '())
                   (cdr wire-path)))
                 (pinned
                  (and original-pinned?
                       ((ledger 'pinned?) origin-value-path))))
            (cond
             (original-proof?
              `((content ,content)
                (pinned? ,(not (not pinned)))
                ,@(if indexed-proof?
                      `((proof-index
                         ,(if history history-origin-index origin-index))) '())
                (proof ,((standard 'serialize) origin-proof))))
             (original-pinned?
              `((content ,content) (pinned? ,(not (not pinned)))))
             (else content))))))

  (define-method (invoke-batch self ledger arguments route history identity signing-key
                               (indexed-proof? #f))
    ;; Resolve one route/history group with one verified terminal multiproof.
    (if (not (and (symbol? identity) (list? route) (pair? route)
                  (list? arguments) (assoc 'paths arguments)
                  (list? (cadr (assoc 'paths arguments)))))
        (error 'authentication-error "Invalid originating batch invocation"))
    (let* ((standard (sync-eval ((self '~field!) 'standard)))
           (origin-index (- ((ledger 'size)) 1))
           (route-response
            ((self 'route) ledger
             `((route-source ()) (route-target ,route) (index ,origin-index)
               ,@(if history `((history-indexes ,history)) '()))))
           (source-route (cadr (assoc 'route-source route-response)))
           (terminal-index (cadr (assoc 'terminal-index route-response)))
           (serialization (cadr (assoc 'object route-response)))
           (source-object ((standard 'deserialize) serialization))
           (target-path ((self '~bridge-route-path) route '()))
           (target-object ((self '~trace-object) ledger origin-index target-path))
           (terminal-object ((self '~get) standard target-object target-path))
           (combined
            (if (and (sync-node? terminal-object)
                     (equal? (sync-digest terminal-object)
                             (sync-digest source-object)))
                ((standard 'deep-merge!) source-object terminal-object)
                (error 'integrity-error
                       "Terminal object is not contained in the origin's committed route")))
           (history-index
            (and history (cadr (assoc 'history-index route-response))))
           (history-head-index
            (and history (cadr (assoc 'history-head-index route-response))))
           (history-origin-index
            (and history
                 (let ((selected (car history)))
                   (if (< selected 0)
                       (+ ((ledger 'size)) selected) selected))))
           (history-path
            (and history ((self '~bridge-route-path) route '() (cdr history))))
           (prepared-history-chain
            (and history
                 ((self '~get) standard target-object
                  (cons history-origin-index history-path))))
           (history-origin-head
            (and history
                 (if (equal? prepared-history-chain '(unknown))
                     ((self '~trace-object)
                      ledger history-origin-index history-path)
                     target-object)))
           (history-chain
            (and history
                 (if (equal? prepared-history-chain '(unknown))
                     ((self '~get) standard history-origin-head history-path)
                     prepared-history-chain)))
           (history-source
            (and history
                 ((standard 'deserialize)
                  (cadr (assoc 'history-object route-response)))))
           (history-merged
            (and history
                 (if (equal? (sync-digest history-chain)
                             (sync-digest history-source))
                     ((standard 'deep-merge!) history-source history-chain)
                     (error 'integrity-error
                            "Ledger history path diverges from the working route"))))
           (data-head (if history history-merged combined))
           (data-head-index (if history history-head-index terminal-index))
           (data-index (if history history-index terminal-index))
           (route-roots (cadr (assoc 'roots route-response)))
           (self-route?
            (and (pair? route-roots) (pair? (cdr route-roots))
                 (equal? (car route-roots) (car (reverse route-roots)))))
           (source-key-path
            (if self-route?
                '(*crypto* interface public-key)
                ((self '~bridge-route-path) (reverse source-route)
                 '(*crypto* interface public-key))))
           (source-key ((self '~get) standard combined source-key-path))
           (_
            (if (byte-vector? source-key) #t
                (error 'bridge-error
                       "Federation route is not ready: reverse interface key is unavailable")))
           (endpoint ((self '~get) standard combined
                      '(*crypto* interface endpoint)))
           (audience ((self '~get) standard combined
                      '(*crypto* journal identity id)))
           (private-key
            (and (pair? signing-key)
                 (equal? source-key (car signing-key))
                 (cdr signing-key)))
           (wire-paths
            (map
             (lambda (path)
               (if (not history) path
                   (cond
                    ((and (pair? path) (integer? (car path)) (< (car path) 0))
                     (cons data-index (cdr path)))
                    ((and (pair? path) (integer? (car path)))
                     (if (> (car path) data-index)
                         (error 'integrity-error
                                "Requested index is newer than the Ledger history path")
                         path))
                    (else (cons data-index path)))))
             (cadr (assoc 'paths arguments))))
           (wire-arguments
            ((self '~alist-set)
             ((self '~alist-set)
              ((self '~alist-set) arguments 'paths wire-paths)
              'pinned? #f)
             'proof? #t))
           (unsigned
            `((identity ,identity)
              (route-source ,source-route)
              (route-target ())
              (object ,serialization)
              (terminal-index ,terminal-index)
              (self-route? ,self-route?)
              ,@(if history
                    `((data-object ,((standard 'serialize) data-head))
                      (data-head-index ,data-head-index)) '())
              (audience ,audience)))
           (signature
            (crypto-sign
             (if private-key private-key
                 (error 'authentication-error
                        "Transient interface key does not match terminal source key"))
             (expression->byte-vector
              ((self '~invocation-message)
               'resolve-batch wire-arguments unsigned))))
           (response
            ((self '~remote)
             endpoint
             `((function resolve-batch)
               (arguments ,wire-arguments)
               (invocation (,@unsigned (signature ,signature))))))
           (verified-response
            ((self '~verify-resolve-batch-response)
             ledger data-head wire-arguments response))
           (results (cadr (assoc 'results verified-response))))
      (if (not indexed-proof?) results
          (let* ((terminal-proof
                  ((standard 'deserialize)
                   (cadr (assoc 'proof verified-response))))
                 (origin-object-path
                  (if history
                      (cons history-origin-index history-path)
                      target-path))
                 (origin-base
                  ((standard 'deep-slice!)
                   (if history history-origin-head target-object)
                   ((self '~ledger-path) origin-object-path)))
                 (origin-proof
                  ((standard 'deep-set!)
                   origin-base ((self '~ledger-path) origin-object-path)
                   terminal-proof)))
            `((results ,results)
              (proof ,((standard 'serialize) origin-proof))
              (proof-index ,(if history history-origin-index origin-index)))))))

  (define-method (authenticate self ledger invocation)
    ;; Authenticate one incoming terminal invocation and return inert context.
    (if (not (and (list? invocation) (assoc 'function invocation)
                  (assoc 'arguments invocation) (assoc 'invocation invocation)))
        (error 'authentication-error "Invalid terminal invocation envelope"))
    (let* ((standard (sync-eval ((self '~field!) 'standard)))
           (operation (cadr (assoc 'function invocation)))
           (arguments (cadr (assoc 'arguments invocation)))
           (value (cadr (assoc 'invocation invocation)))
           (identity ((self '~required) value 'identity))
           (route-source ((self '~required) value 'route-source))
           (route-target ((self '~required) value 'route-target))
           (serialization ((self '~required) value 'object))
           (terminal-index ((self '~required) value 'terminal-index))
           (audience ((self '~required) value 'audience))
           (signature ((self '~required) value 'signature))
           (self-route? (and (assoc 'self-route? value)
                             (cadr (assoc 'self-route? value)))))
      (if (or (not (memq operation '(get set! get-batch set-batch! resolve resolve-batch)))
              (not (symbol? identity))
              (not (and (list? route-source) (pair? route-source)))
              (not (equal? route-target '()))
              (not (list? serialization))
              (not (integer? terminal-index)))
          (error 'authentication-error "Invalid terminal invocation envelope"))
      (let* ((key-path
              (if self-route?
                  '(*crypto* interface public-key)
                  ((self '~bridge-route-path)
                   (reverse route-source)
                   '(*crypto* interface public-key))))
             (object
              ((ledger 'read) terminal-index
               ((standard 'deserialize) serialization)
               (cons
                ((self '~ledger-path) key-path)
                (cond
                 ((and (eq? operation 'resolve) (assoc 'path arguments))
                  (list
                   ((self '~ledger-path)
                    (cadr (assoc 'path arguments)))))
                 ((and (eq? operation 'resolve-batch)
                       (assoc 'paths arguments)
                       (list? (cadr (assoc 'paths arguments))))
                  (map (lambda (path) ((self '~ledger-path) path))
                       (cadr (assoc 'paths arguments))))
                 (else '())))))
             (public-key ((self '~get) standard object key-path))
             (local-audience
              (cadr (assoc 'id
                           ((self '~ref) ((ledger 'config))
                            '(public identity))))))
        (if (not (equal? audience local-audience))
            (error 'authentication-error
                   "Federated invocation audience does not match terminal"))
        (if (or (not (byte-vector? public-key))
                (not (crypto-verify
                      public-key signature
                      (expression->byte-vector
                       ((self '~invocation-message)
                        operation arguments value)))))
            (error 'authentication-error
                   "Could not verify federated invocation signature"))
        (let ((data-head
               (and (assoc 'data-object value)
                    (assoc 'data-head-index value)
                    ((ledger 'read)
                     (cadr (assoc 'data-head-index value))
                     ((standard 'deserialize)
                      (cadr (assoc 'data-object value)))
                     (if (eq? operation 'resolve-batch)
                         (map (lambda (path) ((self '~ledger-path) path))
                              (cadr (assoc 'paths arguments)))
                         ((self '~ledger-path)
                          (cadr (assoc 'path arguments))))))))
          `((principal ,((self '~route-principal) route-source identity))
            (context ((latest-index ,(- ((ledger 'size)) 1))
                      (authentication-index ,terminal-index)))
            ,@(if data-head
                  `((data-head ,data-head)
                    (data-head-index
                     ,(cadr (assoc 'data-head-index value))))
                  '()))))))

  ;; Private implementation helpers.

  (define-method (~verify-resolve-batch-response self ledger head arguments response)
    ;; Verify every declared result against one same-head terminal multiproof.
    (let* ((standard (sync-eval ((self '~field!) 'standard)))
           (proof-entry (and (list? response) (assoc 'proof response)))
           (results-entry (and (list? response) (assoc 'results response)))
           (proof (and proof-entry
                       ((standard 'deserialize) (cadr proof-entry))))
           (paths (and (assoc 'paths arguments)
                       (cadr (assoc 'paths arguments))))
           (results (and results-entry (cadr results-entry))))
      (if (not (and proof (list? paths) (list? results)
                    (= (length paths) (length results))
                    (equal? (sync-digest proof) (sync-digest head))))
          (error 'integrity-error
                 "Terminal resolve batch does not match its committed object"))
      (let loop ((paths paths) (results results))
        (if (null? paths) response
            (let* ((value ((self '~get) standard proof (car paths)))
                   (value
                    (if (and (assoc 'expression? arguments)
                             (cadr (assoc 'expression? arguments))
                             (byte-vector? value))
                        (byte-vector->expression value)
                        value))
                   (content-entry
                    (and (list? (car results))
                         (assoc 'content (car results))))
                   (declared (and content-entry (cadr content-entry))))
              (if (not content-entry)
                  (error 'integrity-error
                         "Terminal batch result is malformed"))
              (if (not (equal? value declared))
                  (error 'integrity-error
                         "Terminal batch result does not match its proof"))
              (loop (cdr paths) (cdr results)))))))

  (define-method (~verify-resolve-response self ledger head arguments response)
    (let* ((standard (sync-eval ((self '~field!) 'standard)))
           (proof-entry (and (list? response) (assoc 'proof response)))
           (proof (and proof-entry
                       ((standard 'deserialize) (cadr proof-entry)))))
      (if (not (and proof
                    (equal? (sync-digest proof) (sync-digest head))))
          (error 'integrity-error
                 "Terminal resolve response does not match its committed object"))
      (let* ((merged
              (if (assoc 'ancestor? response) proof
                  ((standard 'deep-merge!) proof head)))
             (path (cadr (assoc 'path arguments)))
             (value ((self '~get) standard merged path))
             (value
              (if (and (assoc 'expression? arguments)
                       (cadr (assoc 'expression? arguments))
                       (byte-vector? value))
                  (byte-vector->expression value)
                  value)))
        (if (not (equal? value (cadr (assoc 'content response))))
            (error 'integrity-error
                   "Terminal resolve response does not match its committed object"))
        response)))

  (define-method (~trace-object self ledger index path)
    (let* ((standard (sync-eval ((self '~field!) 'standard)))
           (resolve-path
            (if (and (pair? path) (integer? (car path)))
                path (cons index path)))
           (attempt ((ledger 'resolve) resolve-path)))
      (if (not (equal? attempt '(unknown)))
          ((standard 'deserialize)
           ((ledger 'trace) (cons index path)))
          (let* ((edge ((ledger 'peer-head) resolve-path index))
                 (response
                  ((self '~remote)
                   (cadr (assoc 'interface edge))
                   `((function trace)
                     (arguments
                      ((index ,(cadr (assoc 'index edge)))
                       (path ,(cadr (assoc 'path edge))))))))
                 (head
                  ((standard 'deserialize) response)))
            ((ledger 'merge-head!)
             `((path ,resolve-path) (index ,index)) head)))))

  (define-method (~identity-verify self identity)
    (let ((id (and (list? identity) (assoc 'id identity)
                   (cadr (assoc 'id identity))))
          (nonce (and (list? identity) (assoc 'nonce identity)
                      (cadr (assoc 'nonce identity)))))
      (if (not (and (list? identity) (= (length identity) 2)
                    (byte-vector? id) (= (length id) 32)
                    (byte-vector? nonce) (= (length nonce) 32)
                    (equal? id
                            (sync-hash
                             (expression->byte-vector
                              (list 'sync-web/journal-id/v1 nonce))))))
          (error 'identity-error
                 "Invalid journal identity commitment: ~S" identity))
      id))

  (define-method (~rotation-verify self identity rotation previous-key)
    (let* ((identity-id ((self '~identity-verify) identity))
           (field (lambda (key) (and (list? rotation) (assoc key rotation)
                                     (cadr (assoc key rotation)))))
           (index (field 'index))
           (previous-index (field 'previous-index))
           (included-previous (field 'previous-key))
           (public-key (field 'public-key))
           (signature (field 'signature)))
      (if (not (and (list? rotation) (= (length rotation) 5)
                    (integer? index) (integer? previous-index)
                    (< previous-index index)
                    (byte-vector? included-previous)
                    (byte-vector? public-key)
                    (byte-vector? signature)))
          (error 'integrity-error
                 "Malformed journal signing-key transition: ~S" rotation))
      (if (not (equal? included-previous previous-key))
          (error 'integrity-error
                 "Journal signing-key transition does not start at accepted key"))
      (if (not (crypto-verify
                previous-key signature
                (expression->byte-vector
                 (list 'sync-web/journal-key-rotation/v1
                       identity-id index previous-index
                       previous-key public-key))))
          (error 'integrity-error
                 "Journal signing-key transition signature does not verify"))
      public-key))

  (define-method (~signature-verify self chain identity-id expected-key accepted-index)
    (let* ((standard (sync-eval ((self '~field!) 'standard)))
           (chain-copy chain)
           (field (lambda* (path (index -1))
                    ((standard 'deep-get) chain `(,index (*crypto* ,@path))))))
      (for-each
       (lambda (path)
         (set! chain-copy
               ((standard 'deep-set!) chain-copy path #u())))
       '((-1 (*crypto* public-key))
         (-1 (*crypto* signature))
         (-1 (*crypto* journal public-key))
         (-1 (*crypto* journal signature))))
      (let* ((head-index
              (- ((standard 'deep-call) chain '()
                  '(lambda (chain) ((chain 'size)))) 1))
             (identity `((id ,(field '(journal identity id)))
                         (nonce ,(field '(journal identity nonce)))))
             (included-identity ((self '~identity-verify) identity))
             (latest-rotation-index (field '(journal latest-rotation-index)))
             (included-key (field '(journal public-key)))
             (signature (field '(journal signature)))
             (authorized-key
              (if (or (not (integer? latest-rotation-index))
                      (< latest-rotation-index -1)
                      (> latest-rotation-index head-index))
                  (error 'integrity-error
                         "Invalid latest journal key rotation index")
                  (if (not expected-key) included-key
                      (let* ((rotations
                              (let gather ((index latest-rotation-index)
                                           (result '()))
                                (if (<= index accepted-index) result
                                    (let* ((rotation
                                            (field '(journal rotation) index))
                                           (included-index
                                            (and (list? rotation)
                                                 (assoc 'index rotation)
                                                 (cadr (assoc 'index rotation))))
                                           (previous-index
                                            (and (list? rotation)
                                                 (assoc 'previous-index rotation)
                                                 (cadr (assoc 'previous-index rotation)))))
                                      (if (not (and (integer? included-index)
                                                    (= included-index index)
                                                    (integer? previous-index)
                                                    (< previous-index index)))
                                          (error 'integrity-error
                                                 "Missing or malformed journal key rotation"))
                                      (gather previous-index
                                              (cons rotation result))))))
                             (verified
                              (let loop ((items rotations)
                                         (public-key expected-key)
                                         (previous-rotation-index #f))
                                (if (null? items) public-key
                                    (let* ((rotation (car items))
                                           (index (cadr (assoc 'index rotation)))
                                           (previous-index
                                            (cadr (assoc 'previous-index rotation))))
                                      (if (if previous-rotation-index
                                              (not (= previous-index
                                                      previous-rotation-index))
                                              (> previous-index accepted-index))
                                          (error 'integrity-error
                                                 "Journal key rotations do not continue checkpoint"))
                                      (loop (cdr items)
                                            ((self '~rotation-verify)
                                             identity rotation public-key)
                                            index))))))
                        verified)))))
        (cond ((and identity-id
                    (not (equal? identity-id included-identity)))
               (error 'integrity-error
                      "Signed journal identity does not match expected identity"))
              ((not (equal? authorized-key included-key))
               (error 'integrity-error
                      "Signed journal key does not continue accepted key"))
              ((not (crypto-verify included-key signature
                                   (sync-digest chain-copy)))
               (error 'integrity-error "Included signature does not verify"))
              (else included-key)))))

  (define-method (~verify-peer-response self response info interface checkpoint)
    (let* ((standard (sync-eval ((self '~field!) 'standard)))
           (chain ((standard 'deserialize) response))
           (size ((standard 'deep-call) chain '()
                  '(lambda (chain) ((chain 'size)))))
           (existing-identity ((self '~required) checkpoint 'identity))
           (identity (if (null? existing-identity)
                         (and (list? info) (assoc 'identity info)
                              (cadr (assoc 'identity info)))
                         existing-identity))
           (identity-id ((self '~identity-verify) identity))
           (bound-id ((self '~required) checkpoint 'identity-id))
           (expected-key ((self '~required) checkpoint 'public-key))
           (accepted-index ((self '~required) checkpoint 'accepted-index)))
      (if (= size 0)
          (error 'bridge-sync-error
                 "A reciprocal bridge requires a signed peer head"))
      (if (and (byte-vector? bound-id) (not (equal? bound-id identity-id)))
          (error 'bridge-name-error
                 "Peer identity does not match permanent alias binding"))
      (if (and (not (eq? ((self '~required) checkpoint 'status) 'active))
               (eq? ((self '~required) checkpoint 'acceptance) 'preapproved)
               (not (equal?
                     (let ((preapproved
                            ((self '~required) checkpoint 'preapproval)))
                       (if (byte-vector? preapproved) preapproved bound-id))
                     identity-id)))
          (error 'bridge-acceptance-error
                 "Reciprocal bridge identity is not preapproved"))
      (let ((public-key
             ((self '~signature-verify)
              chain identity-id
              (if (byte-vector? expected-key) expected-key #f)
              accepted-index)))
        (if (and info
                 (or (not (equal? identity-id
                                  ((self '~identity-verify)
                                   (cadr (assoc 'identity info)))))
                     (not (equal? public-key
                                  (cadr (assoc 'public-key info))))))
            (error 'bridge-name-error
                   "Peer descriptor does not match signed identity/key"))
        (let ((signed-interface
               ((standard 'deep-get)
                chain '(-1 (*crypto* interface endpoint))))
              (info-interface
               (and info (assoc 'interface info)
                    (cadr (assoc 'endpoint
                                 (cadr (assoc 'interface info)))))))
          (if (or (not (equal? signed-interface interface))
                  (and info-interface
                       (not (equal? signed-interface info-interface))))
              (error 'bridge-endpoint-error
                     "Peer interface does not match signed descriptor")))
        `((serialization ,response)
          (identity ,identity)
          (identity-id ,identity-id)
          (public-key ,public-key)
          (index ,(- size 1))
          (checkpoint ,((self '~required) checkpoint 'checkpoint))))))

  (define-method (~route-principal self route identity)
    (cond ((eq? identity '*journal*) (reverse route))
          ((eq? identity '*public*) '(*public*))
          ((symbol? identity) (append (reverse route) `(*state* ,identity)))
          (else (error 'identity-error
                       "Invalid scalar invocation identity: ~S" identity))))

  (define-method (~invocation-message self operation arguments invocation)
    `((function ,operation)
      (arguments ,arguments)
      (invocation ,((self '~alist-set) invocation 'signature #f #t))))

  (define-method (~bridge-route-path self names tail (indexes '()))
    (if (null? names) tail
        (append `(*bridge* ,(car names)
                  ,@(if (and (pair? indexes) (pair? (cdr names)))
                        (list (car indexes)) '()))
                ((self '~bridge-route-path) (cdr names) tail
                 (if (pair? indexes) (cdr indexes) '())))))

  (define-method (~get self standard object path)
    ((standard 'deep-get) object ((self '~ledger-path) path)))

  (define-method (~ledger-path self path)
    ;; Expand a flat public path into the nested object path used by deep-get.
    (let indexed ((segments path) (index -1))
      (cond ((null? segments) `(,index))
            ((integer? (car segments))
             (indexed (cdr segments) (car segments)))
            ((memq (car segments) '(*state* *transition* *crypto*))
             `(,index ,segments))
            ((or (eq? (car segments) '*bridge*) (symbol? (car segments)))
             (let* ((explicit? (eq? (car segments) '*bridge*))
                    (name (if explicit? (cadr segments) (car segments)))
                    (rest ((if explicit? cddr cdr) segments))
                    (edge `(,index (*bridge* ,name chain))))
               (if (null? rest) edge (append edge (indexed rest -1)))))
            (else
             (error 'path-error "Invalid federation path: ~S" path)))))

  (define-method (~remote self endpoint request)
    (let ((response (sync-remote endpoint request)))
      (if (and (pair? response) (eq? (car response) 'error))
          (apply error (cdr response)) response)))

  (define-method (~required self alist key)
    (let ((entry (and (list? alist) (assoc key alist))))
      (if entry (cadr entry)
          (error 'argument-error "Missing federation field: ~S" key))))

  (define-method (~alist-set self alist key value (remove? #f))
    (let loop ((items alist) (out '()) (found? #f))
      (cond ((null? items)
             (reverse (if (or found? remove?) out (cons (list key value) out))))
            ((eq? (caar items) key)
             (loop (cdr items) (if remove? out (cons (list key value) out)) #t))
            (else (loop (cdr items) (cons (car items) out) found?)))))

  (define-method (~ref self value path)
    (if (null? path) value
        (let ((entry (and (list? value) (assoc (car path) value))))
          (if entry ((self '~ref) (cadr entry) (cdr path)) '()))))

  (define-method (~config-get self path)
    ((self '~ref) (byte-vector->expression ((self '~field!) 'config)) path))

  (define-method (~config-set! self path value)
    ((self '~field!) 'config
     (expression->byte-vector
      (let set-path ((config (byte-vector->expression
                              ((self '~field!) 'config)))
                     (path path))
        (if (null? path) value
            (let loop ((items config))
              (cond ((null? items)
                     (if (null? value) '()
                         (list (list (car path)
                                     (set-path '() (cdr path))))))
                    ((eq? (caar items) (car path))
                     (let ((result (set-path (cadar items) (cdr path))))
                       (if (null? result) (cdr items)
                           (cons (list (car path) result) (cdr items)))))
                    (else
                     (cons (car items) (loop (cdr items)))))))))))

  (define-method (~field! self name value)
    (let ((path (case name
                  ((standard) '(1 0))
                  ((config) '(1 1))
                  (else
                   (error 'field-error "Federation field not found: ~S" name)))))
      (if value (set! (self path) value) (self path)))))
