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
    (if (not ((self '~name-admissible?) alias))
        (error 'bridge-name-error "Peer alias must round trip through the expression codec: ~S" alias))
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
    (if (not (and ((self '~name-admissible?) alias) (list? endpoint)
                  (assoc 'interface endpoint) (assoc 'remote-name endpoint)
                  (string? (cadr (assoc 'interface endpoint)))
                  ((self '~name-admissible?) (cadr (assoc 'remote-name endpoint)))))
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
           (_ (if (not (and ((self '~name-admissible?) name)
                            ((self '~name-admissible?) remote-name)))
                  (error 'bridge-name-error
                         "Bridge aliases must round trip through the expression codec")))
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
                 (equal? ((self '~ref) ((self 'peer) name) '(public-key))
                         ((self '~required) verified 'public-key))
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
                                     (cadr (assoc 'history-head-index request)))))
      (if (not (and (list? route-source) (list? route-target)
                    (or (not history-indexes)
                        (and (list? history-indexes)
                             (= (length history-indexes) (+ (length route-target) 1))
                             (let loop ((indexes history-indexes))
                               (or (null? indexes)
                                   (and (integer? (car indexes))
                                        (loop (cdr indexes)))))))))
          (error 'route-error "Invalid federation route/history shape: ~S" request))
      (if (not (and ((self '~route-admissible?) route-source)
                    ((self '~route-admissible?) route-target)))
          (error 'bridge-name-error
                 "Federation route aliases must round trip through the expression codec"))
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
                       '()))))))
          (let* ((index (if (< index 0) (- ((ledger 'size)) 1) index))
                 (key-path ((self '~bridge-route-path)
                            (reverse route-source)
                            '(*crypto* interface public-key)))
                 (key-object
                  ((self '~trace-object) ledger index key-path))
                 (key-value ((self '~get) standard key-object key-path))
                 (paths (list '(*crypto* interface public-key)
                              '(*crypto* interface endpoint)
                              '(*crypto* journal key-derivation-salt)))
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
    (if (not (memq operation '(put! copy! use! put-batch! copy-batch! use-batch! run! retrieve)))
        (error 'api-error "Function is not available through federation: ~S" operation))
    (if (not (and (symbol? identity) (list? route) (pair? route)))
        (error 'authentication-error "Invalid originating federation invocation"))
    (if (not ((self '~route-admissible?) route))
        (error 'bridge-name-error
               "Federation route aliases must round trip through the expression codec"))
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
           (source-key-path
            ((self '~bridge-route-path) (reverse source-route)
             '(*crypto* interface public-key)))
           (source-key ((self '~get) standard combined source-key-path))
           (_
            (if (byte-vector? source-key) #t
                (error 'bridge-error
                       "Federation route is not ready: reverse interface key is unavailable")))
           (endpoint ((self '~get) standard combined '(*crypto* interface endpoint)))
           (private-key
            (and (pair? signing-key)
                 (equal? source-key (car signing-key))
                 (cdr signing-key)))
           (retrieve? (eq? operation 'retrieve))
           (original-proof? proof-requested?)
           (original-pinned?
            (and (assoc 'pinned? arguments)
                 (cadr (assoc 'pinned? arguments))))
           (wire-arguments
            (if retrieve?
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
                       ,@(if history
                             `((data-object
                                ,((standard 'serialize) data-head))
                               (data-head-index ,data-head-index))
                             '())
                       ))
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
      ;; Retrieve responses carry proof and are re-anchored by the origin.
      (if (not retrieve?) response
          (let* ((verified-response
                  ((self '~verify-retrieve-response)
                   ledger data-head wire-arguments response (not history)))
                 (content (cadr (assoc 'content verified-response)))
                 (indexes
                  (and (assoc 'indexes verified-response)
                       (cadr (assoc 'indexes verified-response))))
                 (terminal-proof
                  ((standard 'deserialize)
                   (cadr (assoc 'proof verified-response))))
                 (origin-object-path
                  (if history
                      (cons history-origin-index history-path)
                      target-path))
                 (origin-base
                  (and (or original-proof? indexed-proof?)
                       ((standard 'deep-slice!)
                        (if history history-origin-head target-object)
                        ((self '~ledger-path) origin-object-path))))
                 (origin-proof
                  (and (or original-proof? indexed-proof?)
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
                ,@(if indexes `((indexes ,indexes)) '())
                ,@(if indexed-proof?
                      `((proof-index
                         ,(if history history-origin-index origin-index))) '())
                (proof ,((standard 'serialize) origin-proof))))
             (indexed-proof?
              `((content ,content)
                ,@(if indexes `((indexes ,indexes)) '())
                (indexed-proof ,((standard 'serialize) origin-proof))))
             (original-pinned?
              `((content ,content) (pinned? ,(not (not pinned)))
                ,@(if indexes `((indexes ,indexes)) '())))
             (indexes `((content ,content) (indexes ,indexes)))
             (else content))))))

  (define-method (invoke-batch self ledger arguments route history identity signing-key
                               (indexed-proof? #f))
    ;; Retrieve one route/history group with one verified terminal multiproof.
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
           (source-key-path
            ((self '~bridge-route-path) (reverse source-route)
             '(*crypto* interface public-key)))
           (source-key ((self '~get) standard combined source-key-path))
           (_
            (if (byte-vector? source-key) #t
                (error 'bridge-error
                       "Federation route is not ready: reverse interface key is unavailable")))
           (endpoint ((self '~get) standard combined
                      '(*crypto* interface endpoint)))
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
              ,@(if history
                    `((data-object ,((standard 'serialize) data-head))
                      (data-head-index ,data-head-index)) '())
              ))
           (signature
            (crypto-sign
             (if private-key private-key
                 (error 'authentication-error
                        "Transient interface key does not match terminal source key"))
             (expression->byte-vector
              ((self '~invocation-message)
               'retrieve-batch wire-arguments unsigned))))
           (response
            ((self '~remote)
             endpoint
             `((function retrieve-batch)
               (arguments ,wire-arguments)
               (invocation (,@unsigned (signature ,signature))))))
           (verified-response
            ((self '~verify-retrieve-batch-response)
             ledger data-head wire-arguments response (not history)))
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
           (signature ((self '~required) value 'signature))
           (retained-provider?
            (and (memq operation '(retrieve retrieve-batch))
                 (not (assoc 'data-object value))))
           (provider-paths
            (cond ((eq? operation 'retrieve)
                   (and (assoc 'path arguments)
                        (list (cadr (assoc 'path arguments)))))
                  ((eq? operation 'retrieve-batch)
                   (and (assoc 'paths arguments)
                        (cadr (assoc 'paths arguments))))
                  (else '()))))
      (if (or (not (memq operation '(put! copy! use! put-batch! copy-batch! use-batch! run! retrieve retrieve-batch)))
              (not (symbol? identity))
              (not (and (list? route-source) (pair? route-source)))
              (not ((self '~route-admissible?) route-source))
              (not (equal? route-target '()))
              (not (list? serialization))
              (not (integer? terminal-index)))
          (error 'authentication-error "Invalid terminal invocation envelope"))
      (let* ((key-path
              ((self '~bridge-route-path) (reverse route-source)
               '(*crypto* interface public-key)))
             (supplied-object ((standard 'deserialize) serialization))
             (operation-paths
              (cond
               ((and (eq? operation 'retrieve) (assoc 'path arguments))
                (list ((self '~ledger-path) (cadr (assoc 'path arguments)))))
               ((and (eq? operation 'retrieve-batch)
                     (assoc 'paths arguments)
                     (list? (cadr (assoc 'paths arguments))))
                (map (lambda (path) ((self '~ledger-path) path))
                     (cadr (assoc 'paths arguments))))
               (else '())))
             (object
              ((ledger 'read) terminal-index supplied-object
               (if retained-provider?
                   (list ((self '~ledger-path) key-path))
                   (cons ((self '~ledger-path) key-path) operation-paths))))
             (public-key ((self '~get) standard object key-path)))
        (if (or (not (byte-vector? public-key))
                (not (crypto-verify
                      public-key signature
                      (expression->byte-vector
                       ((self '~invocation-message)
                        operation arguments value)))))
            (error 'authentication-error
                   "Could not verify federated invocation signature"))
        (let* ((resolved
                (and retained-provider?
                     (begin
                       (if (not (and (list? provider-paths)
                                     (pair? provider-paths)))
                           (error 'argument-error
                                  "Retained retrieval requires committed paths"))
                       ((ledger 'read)
                        terminal-index supplied-object
                        `((paths ,operation-paths)
                          (resolve-latest-perm? #t))))))
               (resolved-paths
                (and resolved (list? resolved)
                     (assoc 'paths resolved)
                     (cadr (assoc 'paths resolved))))
               (provider-paths
                (if retained-provider?
                    (if (and (list? resolved) (= (length resolved) 2)
                             (assoc 'object resolved) (assoc 'paths resolved)
                             (sync-node? (cadr (assoc 'object resolved)))
                             (list? resolved-paths)
                             (= (length resolved-paths)
                                (length provider-paths)))
                        (map (lambda (path)
                               ((self '~public-retained-path) path))
                             resolved-paths)
                        (error 'integrity-error
                               "Invalid resolved retained paths"))
                    provider-paths)))
          (if retained-provider?
              (for-each
               (lambda (path)
                 (if (not ((ledger 'pinned?) path))
                     (error 'availability-error
                            "Responder has not permanently retained path: ~S" path)))
               provider-paths))
          (let* ((data-head
                  (if retained-provider?
                      (cadr (assoc 'object resolved))
                      (and (assoc 'data-object value)
                           (assoc 'data-head-index value)
                           ((ledger 'read)
                            (cadr (assoc 'data-head-index value))
                            ((standard 'deserialize)
                             (cadr (assoc 'data-object value)))
                            (if (eq? operation 'retrieve-batch)
                                (map (lambda (path) ((self '~ledger-path) path))
                                     (cadr (assoc 'paths arguments)))
                                ((self '~ledger-path)
                                 (cadr (assoc 'path arguments))))))))
                 (data-head-index
                  (if retained-provider? terminal-index
                      (and data-head (cadr (assoc 'data-head-index value))))))
            `((principal ,((self '~route-principal) route-source identity))
              (context ((latest-index ,(- ((ledger 'size)) 1))
                        (authentication-index ,terminal-index)))
              ,@(if retained-provider?
                    `((provider-paths ,provider-paths)) '())
              ,@(if data-head
                    `((data-head ,data-head)
                      (data-head-index ,data-head-index))
                    '())))))))

  ;; Private implementation helpers.

  (define-method (~name-admissible? self name)
    ;; Alias symbols must survive the exact durable expression codec canonically.
    (and (symbol? name)
         (let* ((encoded (expression->byte-vector name))
                (decoded (byte-vector->expression encoded)))
           (and (symbol? decoded)
                (equal? (symbol->string decoded) (symbol->string name))
                (equal? encoded (expression->byte-vector decoded))))))

  (define-method (~route-admissible? self route)
    (and (list? route)
         (let loop ((route route))
           (or (null? route)
               (and ((self '~name-admissible?) (car route))
                    (loop (cdr route)))))))

  (define-method (~public-retained-path self path)
    (let loop ((path path) (result '()))
      (if (null? path) result
          (let ((segment (car path)))
            (cond ((integer? segment)
                   (loop (cdr path) (append result (list segment))))
                  ((and (list? segment) (= (length segment) 3)
                        (eq? (car segment) '*bridge*)
                        (eq? (caddr segment) 'chain))
                   (loop (cdr path)
                         (append result (list '*bridge* (cadr segment)))))
                  ((and (list? segment) (pair? segment))
                   (loop (cdr path) (append result segment)))
                  (else
                   (error 'integrity-error
                          "Invalid resolved retained path: ~S" path)))))))

  (define-method (~chain-path? self path)
    (let* ((path ((self '~ledger-path) path))
           (last (and (pair? path) (car (reverse path)))))
      (and (pair? last) (= (length last) 3)
           (eq? (car last) '*bridge*) (eq? (caddr last) 'chain))))

  (define-method (~chain-indices self standard node)
    ;; Negotiate a contained structural inventory with an authenticated Chain.
    (let ((result
           ((standard 'deep-call) node '()
            '(lambda (chain)
               (if (member 'indices (chain '*api*)) ((chain 'indices))
                   (let ((size ((chain 'size))))
                     (if (not (and (integer? size) (>= size 0)))
                         (error 'integrity-error "Invalid historical Chain size"))
                     (let loop ((index 0) (indexes '()) (complete? #t))
                       (if (= index size) `(chain ,(reverse indexes) ,complete?)
                           (let ((available?
                                  (not (equal? ((chain 'get) index) '(unknown)))))
                             (loop (+ index 1)
                                   (if available? (cons index indexes) indexes)
                                   (and complete? available?)))))))))))
      (if (not (and (list? result) (= (length result) 3)
                    (eq? (car result) 'chain) (list? (cadr result))
                    (boolean? (caddr result))
                    (let loop ((indexes (cadr result)) (previous -1))
                      (or (null? indexes)
                          (and (integer? (car indexes)) (> (car indexes) previous)
                               (loop (cdr indexes) (car indexes)))))))
          (error 'integrity-error "Invalid Chain inventory: ~S" result))
      result))

  (define-method (~resolve-retained-proof-path self standard source path)
    ;; Derive every latest selection from one digest-anchored permanent proof.
    (if (not (and (list? path) (pair? path) (integer? (car path))))
        (error 'integrity-error "Invalid retained history path: ~S" path))
    (let* ((requested (car path))
           (inventory
            (and (= requested -1) ((self '~chain-indices) standard source)))
           (indexes (and inventory (cadr inventory)))
           (index (if inventory
                      (if (null? indexes)
                          (error 'availability-error
                                 "Retained Chain has no permanent payload")
                          (car (reverse indexes)))
                      requested))
           (remaining (cdr path)))
      (if (null? remaining) (list index)
          (let ((tree-path (car remaining)))
            (if (not (list? tree-path))
                (error 'integrity-error "Invalid retained history path: ~S" path))
            (if (null? (cdr remaining)) (list index tree-path)
                (let* ((head ((standard 'deep-get) source (list index)))
                       (nested (and (sync-node? head)
                                    ((standard 'deep-get) head (list tree-path)))))
                  (if (or (not (sync-node? nested))
                          (equal? nested '(nothing))
                          (equal? nested '(unknown)))
                      (error 'availability-error
                             "Retained Chain is unavailable: ~S" tree-path))
                  (append
                   (list index tree-path)
                   ((self '~resolve-retained-proof-path)
                    standard nested (cdr remaining)))))))))

  (define-method (~resource-exercise self method arguments)
    ;; Recalculate one opaque resource result from proof-selected material.
    (if (not (list? arguments))
        (error 'argument-error "Resource arguments must be a proper list"))
    (if (and method (not (null? method)) (not (symbol? method)))
        (error 'argument-error "Resource method must be a symbol"))
    (if (and (or (not method) (null? method)) (pair? arguments))
        (error 'argument-error "Blank resource method requires blank arguments"))
    `(lambda (running)
       ,(if (or (not method) (null? method))
            '`((class ,(running '*name*))
               (object-hash ,(sync-digest (running)))
               (code-hash ,(sync-digest (sync-car (running)))))
            `(apply (running ',method) ',arguments))))

  (define-method (~retrieved-value self standard object path method arguments)
    ;; Return inert content or locally exercise an authenticated resource proof.
    (let ((value ((self '~get) standard object path)))
      (if (not (sync-node? value))
          (begin
            (if (or method (pair? arguments))
                (error 'argument-error
                       "Inert retrieve requires blank method and arguments"))
            value)
          (if (and ((self '~chain-path?) path)
                   (or (not method) (null? method)) (null? arguments))
              ((self '~chain-indices) standard value)
              (car
               ((standard 'deep-call!) object ((self '~ledger-path) path)
                ((self '~resource-exercise) method arguments)))))))

  (define-method (~verify-retrieve-batch-response self ledger head arguments response
                                                  (resolve-latest? #f))
    ;; Verify every declared result against one same-head terminal multiproof.
    (let* ((standard (sync-eval ((self '~field!) 'standard)))
           (proof-entry (and (list? response) (assoc 'proof response)))
           (results-entry (and (list? response) (assoc 'results response)))
           (proof (and proof-entry
                       ((standard 'deserialize) (cadr proof-entry))))
           (paths (and (assoc 'paths arguments)
                       (cadr (assoc 'paths arguments))))
           (results (and results-entry (cadr results-entry)))
           (methods (if (assoc 'methods arguments)
                        (cadr (assoc 'methods arguments))
                        (and (list? paths) (map (lambda (path) #f) paths))))
           (resource-arguments
            (if (assoc 'arguments arguments)
                (cadr (assoc 'arguments arguments))
                (and (list? paths) (map (lambda (path) '()) paths))))
           (merged (and proof
                        (equal? (sync-digest proof) (sync-digest head))
                        ((standard 'deep-merge!) proof head))))
      (if (not (and merged (list? paths) (list? results)
                    (list? methods) (list? resource-arguments)
                    (= (length paths) (length results))
                    (= (length paths) (length methods))
                    (= (length paths) (length resource-arguments))))
          (error 'integrity-error
                 "Terminal retrieve batch does not match its committed object"))
      (let loop
          ((paths
            (if resolve-latest?
                (map
                 (lambda (path)
                   ((self '~public-retained-path)
                    ((self '~resolve-retained-proof-path)
                     standard proof ((self '~ledger-path) path))))
                 paths)
                paths))
           (methods methods)
           (resource-arguments resource-arguments)
           (results results))
        (if (null? paths) response
            (let* ((value
                    ((self '~retrieved-value) standard merged (car paths)
                     (car methods) (car resource-arguments)))
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
              (loop (cdr paths) (cdr methods) (cdr resource-arguments)
                    (cdr results)))))))

  (define-method (~verify-retrieve-response self ledger head arguments response
                                            (resolve-latest? #f))
    (let* ((standard (sync-eval ((self '~field!) 'standard)))
           (proof-entry (and (list? response) (assoc 'proof response)))
           (proof (and proof-entry
                       ((standard 'deserialize) (cadr proof-entry)))))
      (if (not (and proof
                    (equal? (sync-digest proof) (sync-digest head))))
          (error 'integrity-error
                 "Terminal retrieve response does not match its committed object"))
      (let* ((merged
              (if (assoc 'ancestor? response) proof
                  ((standard 'deep-merge!) proof head)))
             (path
              (let ((path (cadr (assoc 'path arguments))))
                (if resolve-latest?
                    ((self '~public-retained-path)
                     ((self '~resolve-retained-proof-path)
                      standard proof ((self '~ledger-path) path)))
                    path)))
             (method (and (assoc 'method arguments)
                          (cadr (assoc 'method arguments))))
             (resource-arguments
              (if (assoc 'arguments arguments)
                  (cadr (assoc 'arguments arguments)) '()))
             (value
              (if (assoc 'ancestor? response)
                  ((self '~get) standard merged path)
                  ((self '~retrieved-value) standard merged path method
                   resource-arguments)))
             (value
              (if (and (assoc 'expression? arguments)
                       (cadr (assoc 'expression? arguments))
                       (byte-vector? value))
                  (byte-vector->expression value)
                  value)))
        (if (not (equal? value (cadr (assoc 'content response))))
            (error 'integrity-error
                   "Terminal retrieve response does not match its committed object"))
        response)))

  (define-method (~trace-object self ledger index path)
    (let* ((standard (sync-eval ((self '~field!) 'standard)))
           (retrieve-path
            (if (and (pair? path) (integer? (car path)))
                path (cons index path)))
           (attempt ((ledger 'retrieve) retrieve-path)))
      (if (not (equal? attempt '(unknown)))
          ((standard 'deserialize)
           ((ledger 'trace) (cons index path)))
          (let* ((edge ((ledger 'peer-head) retrieve-path index))
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
             `((path ,retrieve-path) (index ,index)) head)))))

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

  (define-method (~rotation-verify self version identity rotation previous-key)
    (let* ((identity-id (and (= version 1) ((self '~identity-verify) identity)))
           (field (lambda (key) (and (list? rotation) (assoc key rotation)
                                     (cadr (assoc key rotation)))))
           (index (field 'index)) (previous-index (field 'previous-index))
           (included-previous (field 'previous-key)) (public-key (field 'public-key))
           (signature (field 'signature)))
      (if (not (and (memv version '(1 2)) (list? rotation) (= (length rotation) 5)
                    (integer? index) (integer? previous-index) (< previous-index index)
                    (byte-vector? included-previous) (byte-vector? public-key)
                    (byte-vector? signature)))
          (error 'integrity-error "Malformed journal signing-key transition: ~S" rotation))
      (if (not (equal? included-previous previous-key))
          (error 'integrity-error "Journal signing-key transition does not start at accepted key"))
      (if (not (crypto-verify previous-key signature
                (expression->byte-vector
                 (if (= version 1)
                     (list 'sync-web/journal-key-rotation/v1 identity-id index previous-index previous-key public-key)
                     (list 'sync-web/journal-key-rotation/v2 index previous-index previous-key public-key)))))
          (error 'integrity-error "Journal signing-key transition signature does not verify"))
      public-key))

  (define-method (~signature-verify self chain expected-key accepted-index)
    (let* ((standard (sync-eval ((self '~field!) 'standard))) (chain-copy chain)
           (field (lambda* (path (index -1))
                    ((standard 'deep-get) chain `(,index (*crypto* ,@path))))))
      (for-each (lambda (path) (set! chain-copy ((standard 'deep-set!) chain-copy path #u())))
                '((-1 (*crypto* public-key)) (-1 (*crypto* signature))
                  (-1 (*crypto* journal public-key)) (-1 (*crypto* journal signature))))
      (let* ((head-index (- ((standard 'deep-call) chain '() '(lambda (chain) ((chain 'size)))) 1))
             (version (let ((value (field '(journal format-version))))
                        (if (equal? value '(nothing)) 1 value)))
             (identity (and (= version 1)
                            `((id ,(field '(journal identity id)))
                              (nonce ,(field '(journal identity nonce))))))
             (_ (if (= version 1) ((self '~identity-verify) identity)
                    (let ((salt (field '(journal key-derivation-salt))))
                      (if (not (and (= version 2) (byte-vector? salt) (= (length salt) 32)))
                          (error 'integrity-error "Invalid Journal head format")))))
             (latest-rotation-index (field '(journal latest-rotation-index)))
             (included-key (field '(journal public-key)))
             (signature (field '(journal signature)))
             (authorized-key
              (if (or (not (integer? latest-rotation-index)) (< latest-rotation-index -1)
                      (> latest-rotation-index head-index))
                  (error 'integrity-error "Invalid latest journal key rotation index")
                  (if (not expected-key) included-key
                      (let* ((rotations
                              (let gather ((index latest-rotation-index) (result '()))
                                (if (<= index accepted-index) result
                                    (let* ((rotation (field '(journal rotation) index))
                                           (included-index (and (list? rotation) (assoc 'index rotation)
                                                                (cadr (assoc 'index rotation))))
                                           (previous-index (and (list? rotation) (assoc 'previous-index rotation)
                                                                (cadr (assoc 'previous-index rotation)))))
                                      (if (not (and (integer? included-index) (= included-index index)
                                                    (integer? previous-index) (< previous-index index)))
                                          (error 'integrity-error "Missing or malformed journal key rotation"))
                                      (gather previous-index (cons (list index rotation) result))))))
                             (verified
                              (let loop ((items rotations) (public-key expected-key) (prior #f))
                                (if (null? items) public-key
                                    (let* ((index (caar items)) (rotation (cadar items))
                                           (previous-index (cadr (assoc 'previous-index rotation)))
                                           (rotation-version
                                            (let ((value (field '(journal format-version) index)))
                                              (if (equal? value '(nothing)) 1 value)))
                                           (rotation-identity
                                            (and (= rotation-version 1)
                                                 `((id ,(field '(journal identity id) index))
                                                   (nonce ,(field '(journal identity nonce) index))))))
                                      (if (if prior (not (= previous-index prior)) (> previous-index accepted-index))
                                          (error 'integrity-error "Journal key rotations do not continue checkpoint"))
                                      (loop (cdr items)
                                            ((self '~rotation-verify) rotation-version rotation-identity rotation public-key)
                                            index))))))
                        verified)))))
        (if (not (equal? authorized-key included-key))
            (error 'integrity-error "Signed journal key does not continue accepted key"))
        (if (not (crypto-verify included-key signature (sync-digest chain-copy)))
            (error 'integrity-error "Included signature does not verify"))
        included-key)))

  (define-method (~verify-peer-response self response info interface checkpoint)
    (let* ((standard (sync-eval ((self '~field!) 'standard)))
           (chain ((standard 'deserialize) response))
           (size ((standard 'deep-call) chain '() '(lambda (chain) ((chain 'size)))))
           (expected-key ((self '~required) checkpoint 'public-key))
           (accepted-index ((self '~required) checkpoint 'accepted-index)))
      (if (= size 0) (error 'bridge-sync-error "A reciprocal bridge requires a signed peer head"))
      (let ((public-key ((self '~signature-verify) chain
                         (if (byte-vector? expected-key) expected-key #f) accepted-index)))
        (if (and (not (eq? ((self '~required) checkpoint 'status) 'active))
                 (eq? ((self '~required) checkpoint 'acceptance) 'preapproved)
                 (not (equal? ((self '~required) checkpoint 'preapproval)
                              (sync-hash public-key))))
            (error 'bridge-acceptance-error
                   "Reciprocal bridge signing key hash is not preapproved"))
        (if (and info (not (equal? public-key (cadr (assoc 'public-key info)))))
            (error 'bridge-name-error "Peer descriptor does not match signed key"))
        (let ((signed-interface ((standard 'deep-get) chain '(-1 (*crypto* interface endpoint))))
              (info-interface (and info (assoc 'interface info)
                                   (cadr (assoc 'endpoint (cadr (assoc 'interface info)))))))
          (if (or (not (equal? signed-interface interface))
                  (and info-interface (not (equal? signed-interface info-interface))))
              (error 'bridge-endpoint-error "Peer interface does not match signed descriptor")))
        `((serialization ,response) (public-key ,public-key) (index ,(- size 1))
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
            ((and (eq? (car segments) '*bridge*) (null? (cdr segments)))
             `(,index (*bridge*)))
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
    (if (and (list? path) (>= (length path) 2)
             (memq (car path) '(peers retired))
             (not ((self '~name-admissible?) (cadr path))))
        (error 'bridge-name-error
               "Bridge alias does not round trip through the expression codec: ~S"
               (cadr path)))
    (if (and (list? path) (= (length path) 1)
             (memq (car path) '(peers retired))
             (not (and (list? value)
                       (let valid? ((entries value))
                         (or (null? entries)
                             (and (list? (car entries))
                                  (= (length (car entries)) 2)
                                  ((self '~name-admissible?) (caar entries))
                                  (valid? (cdr entries))))))))
        (error 'bridge-name-error
               "Malformed or inadmissible Federation alias table: ~S" value))
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
